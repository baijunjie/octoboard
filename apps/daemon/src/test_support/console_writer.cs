// A program of the daemon's tests on Windows that writes everything on its standard input to its
// console in one write, less one final line feed (the one a Bash here-string adds): a frame drawn
// at once, as an agent draws one. Git Bash writes to a console in pieces, so a screen a Bash
// stand-in prints with `printf` reaches the daemon cut into several reads, some milliseconds apart.
//
// Given a path, it reads its input first and writes only once that file exists, so that a stand-in
// can start it ahead of the moment it has to draw. It exits with 1 when it could not write, or
// when the file never came.

using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class ConsoleWriter
{
    const int StdOutput = -11;
    const uint EnableVirtualTerminalProcessing = 4;
    // Longer than any test waits for what it draws.
    static readonly TimeSpan GoWait = TimeSpan.FromMinutes(5);

    [DllImport("kernel32.dll")]
    static extern IntPtr GetStdHandle(int which);

    [DllImport("kernel32.dll")]
    static extern bool GetConsoleMode(IntPtr console, out uint mode);

    [DllImport("kernel32.dll")]
    static extern bool SetConsoleMode(IntPtr console, uint mode);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool WriteConsoleW(IntPtr console, string text, uint length, out uint written,
        IntPtr reserved);

    public static int Main(string[] args)
    {
        MemoryStream input = new MemoryStream();
        Console.OpenStandardInput().CopyTo(input);
        byte[] bytes = input.ToArray();
        int length = bytes.Length;
        if (length > 0 && bytes[length - 1] == (byte)'\n')
        {
            length--;
        }
        string text = new UTF8Encoding(false).GetString(bytes, 0, length);
        if (args.Length > 0)
        {
            DateTime deadline = DateTime.UtcNow + GoWait;
            while (!File.Exists(args[0]))
            {
                if (DateTime.UtcNow > deadline)
                {
                    return 1;
                }
                Thread.Sleep(2);
            }
        }
        IntPtr console = GetStdHandle(StdOutput);
        // Git Bash leaves the console's escape sequence processing off for a program that is not
        // its own, which would print the escapes as text; they are the screen's drawing.
        uint mode;
        if (GetConsoleMode(console, out mode))
        {
            SetConsoleMode(console, mode | EnableVirtualTerminalProcessing);
        }
        uint written;
        bool wrote = WriteConsoleW(console, text, (uint)text.Length, out written, IntPtr.Zero);
        return wrote && written == text.Length ? 0 : 1;
    }
}
