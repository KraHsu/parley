"""Native GUI selection/cleanup check; uses fake local CLI processes, no model service."""
import json
import os
import pathlib
import subprocess
import tempfile
import time
from installed_upgrade import Browser, request


def wait(check, label):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(.1)
    raise AssertionError(label)


def script(browser, source, *args):
    return request("POST", browser.path + "/execute/sync", {"script": source, "args": list(args)})


children = []
browser = None
try:
    with tempfile.TemporaryDirectory(prefix="parley-management-") as tmp:
        root = pathlib.Path(tmp)
        gui = root / "fake-gui"
        gui.write_text("#!/bin/sh\nexit 0\n")
        gui.chmod(0o700)
        native = root / "native"
        native.write_text('''#!/bin/sh
printf '%s' "$2" > "$PARLEY_FIXTURE_PATH"
printf '%s' "{\\"session_id\\":\\"$PARLEY_FIXTURE_NAME\\",\\"hook_event_name\\":\\"UserPromptSubmit\\",\\"prompt\\":\\"$PARLEY_FIXTURE_NAME\\"}" | /usr/lib/parley-desktop/parley-cli --terminal-event "$2"
read -r finish
''')
        native.chmod(0o700)
        ids = []
        for name in ["First French session", "Second French session"]:
            marker = root / str(len(ids))
            child = subprocess.Popen(["/usr/lib/parley-desktop/parley-cli", "--backend", "claude-code", "--claude", str(native), "--gui-bin", str(gui)],
                cwd=root, env={**os.environ, "PARLEY_FIXTURE_PATH": str(marker), "PARLEY_FIXTURE_NAME": name}, stdin=subprocess.PIPE, stderr=subprocess.DEVNULL)
            children.append(child)
            directory = wait(lambda: marker.read_text() if marker.exists() and marker.stat().st_size else None, "native CLI directory")
            wait(lambda: (pathlib.Path(directory) / "context.json").exists(), "initial hook")
            ids.append(pathlib.Path(directory).name)
        browser = Browser()
        listed = browser.call("companion_list")
        assert all(next(s for s in listed["sessions"] if s["id"] == id)["status"] == "running" for id in ids)
        script(browser, "document.querySelectorAll('dialog[open]').forEach(d=>d.close()); document.querySelector('.preferences-dialog').showModal(); document.querySelector('.companion-sessions details').open=true;")
        for id, name in zip(ids, ["First French session", "Second French session"]):
            script(browser, "const s=document.querySelector('.companion-sessions select'); s.value=arguments[0]; s.dispatchEvent(new Event('change',{bubbles:true}));", id)
            wait(lambda: script(browser, "return document.querySelector('.terminal-context select')?.selectedOptions[0]?.textContent?.includes(arguments[0])", name), "selected thread rendered " + name)
        assert script(browser, "const s=document.querySelector('.companion-sessions'); return s.scrollWidth <= s.clientWidth + 1;"), "Long session paths must fit the panel"
        assert all(c.poll() is None for c in children), "Switching GUI must retain both CLI processes"
        # Running cache cannot be removed even through a direct IPC call.
        try:
            browser.call("companion_remove", id=ids[0])
            raise AssertionError("Running cache removed")
        except RuntimeError as error:
            assert "运行" in str(error)
        children[0].stdin.write(b"exit\n")
        children[0].stdin.flush()
        children[0].wait(timeout=5)
        wait(lambda: script(browser, "return Array.from(document.querySelector('.companion-sessions select').options).find(o=>o.value===arguments[0])?.textContent.includes('已结束')", ids[0]), "ended status polled in GUI")
        # Exercise the actual GUI confirmation and cache button.
        script(browser, "window.confirm=()=>true; const row=Array.from(document.querySelectorAll('.companion-cache-row')).find(r=>r.textContent.includes(arguments[0])); row.querySelector('button').click();", ids[0][:8])
        wait(lambda: not any(s["id"] == ids[0] for s in browser.call("companion_list")["sessions"]), "ended cache removed")
        assert children[1].poll() is None
        assert browser.call("companion_list")["selectedId"] == ids[1]
        # Detaching removes stale context and the terminal-context widget.
        script(browser, "const s=document.querySelector('.companion-sessions select'); s.value=''; s.dispatchEvent(new Event('change',{bubbles:true}));")
        wait(lambda: script(browser, "return !document.querySelector('.terminal-context')"), "detached GUI clears context")
        print(json.dumps({"guiSelection": True, "twoNativeProcessesPreserved": True, "endedStatus": True, "runningCleanupRefused": True, "guiCacheCleanup": True, "detach": True}))
finally:
    if browser:
        browser.close()
    for child in children:
        if child.poll() is None:
            child.stdin.write(b"exit\n")
            child.stdin.flush()
            child.wait(timeout=5)
