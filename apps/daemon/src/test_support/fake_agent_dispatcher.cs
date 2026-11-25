// A fake agent CLI of the daemon's tests on Windows, compiled with `BASH_PATH` replaced by Git
// Bash's path: it runs the companion file beside the name it was started by, `<name>.body`, with
// Bash, handing it its arguments as the C runtime splits them and its own standard handles,
// and exits with Bash's exit code. It waits for Bash rather than leaving it to run alone, so that
// whatever ends the fake's process tree, or waits for the fake, reaches Bash too.

using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class FakeAgentDispatcher
{
    const string Bash = @"BASH_PATH";

    const uint UseStdHandles = 0x100;
    const uint HandleFlagInherit = 1;
    const uint Infinite = 0xFFFFFFFF;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct StartupInfo
    {
        public int Size;
        public string Reserved, Desktop, Title;
        public uint X, Y, XSize, YSize, XCountChars, YCountChars, FillAttribute, Flags;
        public ushort ShowWindow, Reserved2Size;
        public IntPtr Reserved2, StdInput, StdOutput, StdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct ProcessInformation
    {
        public IntPtr Process, Thread;
        public uint ProcessId, ThreadId;
    }

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool CreateProcessW(string application, StringBuilder commandLine,
        IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint flags,
        IntPtr environment, string directory, ref StartupInfo startup,
        out ProcessInformation started);

    [DllImport("kernel32.dll")]
    static extern IntPtr GetStdHandle(int which);

    [DllImport("kernel32.dll")]
    static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);

    [DllImport("kernel32.dll")]
    static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll")]
    static extern bool GetExitCodeProcess(IntPtr process, out uint code);

    // The standard handle `which`, made inheritable so Bash gets it whatever the starter made it.
    static IntPtr Inherited(int which)
    {
        IntPtr handle = GetStdHandle(which);
        SetHandleInformation(handle, HandleFlagInherit, HandleFlagInherit);
        return handle;
    }

    // `word` as one argument of a command line that a program of Git Bash's (MSYS2) splits: in
    // double quotes, so nothing in it is globbed, with a backslash before each backslash and each
    // double quote, which MSYS2 takes as escapes there; it does not split as the C runtime does,
    // which leaves a backslash that no double quote follows as it is.
    static string Quoted(string word)
    {
        return "\"" + word.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
    }

    public static int Main(string[] args)
    {
        string self = System.Reflection.Assembly.GetEntryAssembly().Location;
        string body = Path.ChangeExtension(self, ".body");
        StartupInfo startup = new StartupInfo();
        startup.Size = Marshal.SizeOf(typeof(StartupInfo));
        startup.Flags = UseStdHandles;
        startup.StdInput = Inherited(-10);
        startup.StdOutput = Inherited(-11);
        startup.StdError = Inherited(-12);
        string program = Bash;
        StringBuilder command;
        string fixture = Path.ChangeExtension(self, ".fixture");
        if (File.Exists(fixture))
        {
            string[] launch = File.ReadAllLines(fixture);
            program = launch[0];
            command = new StringBuilder("\"" + program + "\" " + launch[1]);
        }
        else
        {
            command = new StringBuilder(Quoted(Bash) + " " + Quoted(body));
            foreach (string arg in args)
            {
                command.Append(' ').Append(Quoted(arg));
            }
        }
        ProcessInformation started;
        if (!CreateProcessW(program, command, IntPtr.Zero, IntPtr.Zero, true, 0, IntPtr.Zero, null,
            ref startup, out started))
        {
            Console.Error.WriteLine("the fake agent could not start its program: error "
                + Marshal.GetLastWin32Error());
            return 127;
        }
        WaitForSingleObject(started.Process, Infinite);
        uint code;
        GetExitCodeProcess(started.Process, out code);
        return unchecked((int)code);
    }
}
