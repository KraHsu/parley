"""Exercise installed binaries through WebKit WebDriver and an isolated Secret Service.

Run `seed` with the old package, then `verify` after apt upgrades the same installation.
Requires an isolated home/data directory, WebKitWebDriver :4444, and an unlocked keyring.
All model traffic stays on loopback. Never run against a personal workspace.
"""
import argparse
import http.server
import json
import pathlib
import sqlite3
import threading
import time
import urllib.request
import uuid


def uid():
    return str(uuid.uuid4())


def request(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request("http://127.0.0.1:4444" + path, data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=45) as response:
        result = json.load(response)["value"]
    if isinstance(result, dict) and "error" in result:
        raise RuntimeError(result)
    return result


class Browser:
    def __init__(self):
        result = request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "webkitgtk:browserOptions": {"binary": "/usr/lib/parley-desktop/parley"}}}})
        self.path = "/session/" + result["sessionId"]
        self.call("storage_load")

    def call(self, command, **args):
        result = request("POST", self.path + "/execute/async", {
            "script": """const [command,args,done]=arguments;
              if(command==='backend_send') args.events='__CHANNEL__:'+window.__TAURI_INTERNALS__.transformCallback(()=>{});
              window.__TAURI_INTERNALS__.invoke(command,args).then(v=>done({ok:v}),e=>done({error:String(e)}));""",
            "args": [command, args]})
        return result.get("ok")

    def close(self):
        request("DELETE", self.path)
        # The close command may return before the process releases SQLite.
        time.sleep(.5)


class Fixture(http.server.BaseHTTPRequestHandler):
    requests = []
    def log_message(self, *args):
        pass

    def do_POST(self):
        assert self.path == "/v1/chat/completions"
        assert self.headers["Authorization"] == "Bearer upgrade-fixture-key"
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.requests.append(body)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        chunk = {"choices": [{"index": 0, "delta": {"content": "Bonjour, welcome back!", "reasoning_content": "fixture continuity"}, "finish_reason": "stop"}]}
        self.wfile.write(("data: " + json.dumps(chunk) + "\n\ndata: [DONE]\n\n").encode())


def snapshot(path):
    with sqlite3.connect("file:" + str(path) + "?mode=ro", uri=True) as db:
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert not db.execute("PRAGMA foreign_key_check").fetchall()
        tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        data = {table: sorted(db.execute('SELECT * FROM "' + table + '"').fetchall(), key=repr) for table in tables}
        return json.loads(json.dumps({"version": db.execute("PRAGMA user_version").fetchone()[0], "tables": data}))


def send(browser, state, text):
    profile = state["profile"]
    browser.call("backend_send", request={"backendKind": "openai_compatible", "pane": "main",
        "profileId": profile["id"], "profileRevision": profile["revision"], "conversationId": state["conversation"],
        "messageId": uid(), "text": text, "model": "fixture", "targetLanguage": "fr", "nativeLanguage": "zh-CN", "mode": "conversation", "terminalContext": None})
    deadline = time.monotonic() + 20
    while True:
        conversation = browser.call("storage_read", id=state["conversation"])
        if conversation["messages"] and conversation["messages"][-1]["role"] == "assistant" and conversation["messages"][-1]["status"] in ["complete", "failed", "interrupted"]:
            break
        assert time.monotonic() < deadline, "Timed out waiting for the stored reply"
        time.sleep(.1)
    assert conversation["messages"][-1]["text"] == "Bonjour, welcome back!"
    assert conversation["messages"][-1]["status"] == "complete"
    return conversation


def seed(browser):
    profile = browser.call("backend_save_profile", request={"id": None, "expectedRevision": None,
        "config": {"name": "Upgrade fixture", "kind": "openai_compatible", "provider": "custom",
                   "endpoint": "http://127.0.0.1:4891/v1", "binaryPath": "", "enabled": True}})
    status = browser.call("backend_set_credential", profileId=profile["id"], revision=profile["revision"], key="upgrade-fixture-key", persist=True)
    assert status == {"configured": True, "persistence": "system"}, status
    conversation = browser.call("storage_create", id=uid(), pane="main", profileId=profile["id"], sourceId=None)
    state = {"profile": profile, "conversation": conversation["id"], "words": []}
    send(browser, state, "Bonjour, I am learning French.")
    preferences = browser.call("storage_load")["preferences"]
    preferences.update(mainId=conversation["id"], mainModel="fixture", targetLanguage="fr")
    browser.call("storage_save", preferences=preferences, drafts=[{"id": conversation["id"], "text": "An unsent question"}])
    for text, meaning in [("bonjour", "你好"), ("au revoir", "再见")]:
        fields = {"text": text, "meaning": meaning, "language": "fr", "languageLabel": "", "meaningLanguage": "zh-CN", "kind": "phrase", "note": "Upgrade preservation"}
        source = {"sourceKind": "manual", "conversationId": None, "messageId": None, "threadId": None,
            "turnId": None, "itemId": None, "role": "manual", "selectedText": text, "snapshot": text + "!",
            "start": 0, "end": len(text), "locatorVersion": 1, "truncated": False}
        entry = browser.call("vocabulary_save", request={"requestId": uid(), "id": None, "expectedRevision": None,
            "fields": fields, "occurrence": source, "draftId": None, "tags": ["upgrade"]})["entry"]
        state["words"].append(entry["id"])
        for direction in ["recognition", "production"]:
            card = browser.call("vocabulary_card_save", request={"requestId": uid(), "entryId": entry["id"], "direction": direction, "suspended": False, "reset": False, "expectedRevision": None})
            grade = browser.call("vocabulary_review_grade", request={"requestId": uid(), "cardId": card["id"], "expectedRevision": card["revision"], "rating": "remembered"})
            if direction == "production":
                browser.call("vocabulary_review_undo", request={"requestId": uid(), "reviewId": grade["reviewId"]})
        if text == "au revoir":
            latest = browser.call("vocabulary_get", id=entry["id"])
            browser.call("vocabulary_trash", request={"requestId": uid(), "id": entry["id"], "expectedRevision": latest["revision"]})
    browser.call("vocabulary_save_draft", draft={"id": uid(), "entryId": None, "baseRevision": None,
        "fields": fields, "occurrence": None, "allowDuplicate": False, "requestId": uid(), "tagText": "draft"})
    return state


def verify(browser, baseline, state, path):
    migrated = snapshot(path)
    assert migrated["version"] == 9, migrated["version"]
    for table, rows in baseline["tables"].items():
        assert migrated["tables"][table] == rows, "Changed during upgrade: " + table
    backups = list(path.parent.glob("parley-before-v9-*.sqlite3"))
    assert len(backups) == 1, backups
    assert snapshot(backups[0]) == baseline, "Pre-upgrade backup differs"
    p = state["profile"]
    assert browser.call("backend_credential_status", profileId=p["id"])["configured"]
    assert len(send(browser, state, "Continue after the package upgrade.")["messages"]) == 4
    assert any("Bonjour, I am learning French." in str(m) for m in Fixture.requests[-1]["messages"])
    assert len(browser.call("vocabulary_load_drafts")) == 1
    # Persist-to-session transition must delete the real saved key, retaining identity.
    browser.call("backend_set_credential", profileId=p["id"], revision=p["revision"], key="upgrade-fixture-key", persist=False)
    browser.close()
    browser = Browser()
    assert not browser.call("backend_credential_status", profileId=p["id"])["configured"]
    browser.call("backend_set_credential", profileId=p["id"], revision=p["revision"], key="upgrade-fixture-key", persist=False)
    assert len(send(browser, state, "Continue after re-entering the same key.")["messages"]) == 6
    browser.close()
    assert len(list(path.parent.glob("parley-before-v9-*.sqlite3"))) == 1
    assert b"upgrade-fixture-key" not in path.read_bytes()
    return {"schema": 9, "preservedTables": len(baseline["tables"]), "preUpgradeBackup": True,
        "realSystemCredential": True, "sameConversationAfterUpgradeAndKeyReentry": True, "requests": len(Fixture.requests)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["seed", "verify"])
    parser.add_argument("--artifacts", required=True, type=pathlib.Path)
    args = parser.parse_args()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 4891), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    browser = Browser()
    runtime = browser.call("get_runtime_info")
    path = pathlib.Path(browser.call("storage_load")["path"])
    try:
        if args.mode == "seed":
            assert runtime["appVersion"] in ["0.4.0-alpha.1", "0.4.0-alpha.2"]
            state = seed(browser)
            browser.close()
            baseline = snapshot(path)
            (args.artifacts / "baseline.json").write_text(json.dumps({"state": state, "baseline": baseline, "runtime": runtime}))
            print(json.dumps({"oldVersion": runtime["appVersion"], "schema": baseline["version"], "seededTables": len(baseline["tables"])}))
        else:
            assert runtime["appVersion"] == "0.4.0-alpha.3"
            data = json.loads((args.artifacts / "baseline.json").read_text())
            result = verify(browser, data["baseline"], data["state"], path)
            result.update(oldVersion=data["runtime"]["appVersion"], newVersion=runtime["appVersion"])
            (args.artifacts / "result.json").write_text(json.dumps(result, indent=2))
            print(json.dumps(result))
    finally:
        try:
            browser.close()
        except Exception:
            pass
        server.shutdown()
