using System;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Runtime.InteropServices;
using AlphaCode.Desktop;

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
        string collectDiagnostics = null;
        for (var i = 0; i < args.Length; i++)
        {
            if (args[i].Equals("-Executable", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length) executable = args[++i];
            if (args[i].Equals("-Preview", StringComparison.OrdinalIgnoreCase)) preview = true;
            if (args[i].Equals("-CollectDiagnostics", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length) collectDiagnostics = args[++i];
        }
        using (var resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("AlphaCode.UI"))
        {
            if (resource == null) return 2;
            using (var reader = new StreamReader(resource))
            {
            var script = reader.ReadToEnd();
            GuiDiagnostics.Initialize(Assembly.GetExecutingAssembly(), script);
            GuiDiagnostics.InstallProcessHandlers();
            if (collectDiagnostics != null) {
                try { GuiDiagnostics.Export(collectDiagnostics); return 0; }
                catch (Exception error) { GuiDiagnostics.Write("host", "CollectDiagnostics", error, null, null); return 1; }
            }
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
                    powershell.AddScript(script).AddParameter("Executable", executable);
                    if (preview) powershell.AddParameter("Preview", true);
                    try
                    {
                        powershell.Invoke();
                    }
                    catch (Exception error)
                    {
                        GuiDiagnostics.Write("host", "PowerShell.Invoke", error, null, null);
                        Console.Error.WriteLine("AlphaCode GUI failed. Redacted diagnostics are saved in LocalAppData/AlphaCode.");
                        return 1;
                    }

                    // HadErrors also remains true for terminating cmdlet
                    // errors handled by a script's local try/catch (e.g. a
                    // malformed bridge reply followed by a clean reconnect).
                    // Only unhandled errors remaining in the error stream
                    // make an otherwise completed invocation fail.
                    if (powershell.Streams.Error.Count != 0)
                    {
                        foreach (var error in powershell.Streams.Error)
                            GuiDiagnostics.Write("host", "PowerShell.Streams.Error", error, null, null);
                        return 1;
                    }
                    return 0;
                }
            }
            }
        }
    }
}
