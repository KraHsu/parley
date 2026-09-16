using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text;

namespace ParleyValidation
{
    // Read the Unicode Shell interface directly. WScript.Shell converts paths
    // and arguments through the ANSI code page and loses Chinese characters.
    // https://learn.microsoft.com/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getpath
    [ComImport, Guid("00021401-0000-0000-C000-000000000046")]
    internal class ShellLink { }

    [ComImport, Guid("000214F9-0000-0000-C000-000000000046")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IShellLinkW
    {
        void GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder path, int length, IntPtr data, uint flags);
        void GetIDList(out IntPtr idList);
        void SetIDList(IntPtr idList);
        void GetDescription([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder text, int length);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string text);
        void GetWorkingDirectory([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder path, int length);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string path);
        void GetArguments([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder arguments, int length);
    }

    public sealed class Shortcut
    {
        public string TargetPath { get; private set; }
        public string Arguments { get; private set; }

        public static Shortcut Read(string file)
        {
            var link = (IShellLinkW)new ShellLink();
            try
            {
                ((IPersistFile)link).Load(file, 0);
                var path = new StringBuilder(32768);
                var arguments = new StringBuilder(32768);
                link.GetPath(path, path.Capacity, IntPtr.Zero, 4); // SLGP_RAWPATH
                link.GetArguments(arguments, arguments.Capacity);
                return new Shortcut { TargetPath = path.ToString(), Arguments = arguments.ToString() };
            }
            finally
            {
                Marshal.FinalReleaseComObject(link);
            }
        }
    }
}
