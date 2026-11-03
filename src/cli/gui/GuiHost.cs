using System;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Runtime.InteropServices;

internal static class GuiHost
{
    // Windows groups taskbar buttons, jump lists and notifications by
    // AppUserModelID. A process without one is grouped by executable path, which
    // means the shortcut, the CLI and the host can appear as three separate
    // applications. This value must match the one the installer records under
    // HKCU\Software\Classes\Applications\<exe>\AppUserModelID
    // (scripts/install.ps1: $script:AlphacodeAppId).
    private const string AppUserModelId = "AlphaCode.Terminal";

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    private static extern void SetCurrentProcessExplicitAppUserModelID(
        [MarshalAs(UnmanagedType.LPWStr)] string appId);

    /// <summary>
    /// Give the process a stable taskbar identity.
    /// Best effort: an older Windows build, or a locked-down shell, must not stop
    /// the GUI from starting just because grouping cannot be set.
    /// </summary>
    private static void ApplyAppUserModelId()
    {
        try { SetCurrentProcessExplicitAppUserModelID(AppUserModelId); }
        catch { }
    }

    [STAThread]
    private static int Main(string[] args)
    {
        ApplyAppUserModelId();
        var executable = Environment.GetEnvironmentVariable("ALPHACODE_DESKTOP_EXECUTABLE") ?? "alphacode.exe";
        var preview = false;
        for (var i = 0; i < args.Length; i++)
        {
            if (args[i].Equals("-Executable", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length) executable = args[++i];
            if (args[i].Equals("-Preview", StringComparison.OrdinalIgnoreCase)) preview = true;
        }
        using (var resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("AlphaCode.UI"))
        {
            if (resource == null) return 2;
            using (var reader = new StreamReader(resource))
            using (var runspace = RunspaceFactory.CreateRunspace())
            {
                // WPF requires the thread that constructs the Window to be
                // STA. PowerShell's default runspace may execute on an MTA
                // worker even when this host's Main method is marked STA.
                runspace.ApartmentState = System.Threading.ApartmentState.STA;
                runspace.ThreadOptions = PSThreadOptions.ReuseThread;
                runspace.Open();
                using (var powershell = PowerShell.Create())
                {
                    powershell.Runspace = runspace;
                    powershell.AddScript(reader.ReadToEnd()).AddParameter("Executable", executable);
                    if (preview) powershell.AddParameter("Preview", true);
                    try
                    {
                        powershell.Invoke();
                    }
                    catch (Exception error)
                    {
                        Console.Error.WriteLine("AlphaCode GUI PowerShell host failed: " + error);
                        return 1;
                    }

                    if (powershell.HadErrors)
                    {
                        foreach (var error in powershell.Streams.Error)
                            Console.Error.WriteLine("AlphaCode GUI PowerShell error: " + error);
                        return 1;
                    }
                    return 0;
                }
            }
        }
    }
}
