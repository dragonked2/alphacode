using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Management.Automation;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;

namespace AlphaCode.Desktop
{
    // Also loaded by gui.ps1 when run directly. Native CLR handlers must not be
    // PowerShell script blocks: a fatal/background thread may have no runspace.
    public static class GuiDiagnostics
    {
        private const long MaxLogBytes = 1024 * 1024;
        private static readonly object Gate = new object();
        private static readonly TimeSpan RegexTimeout = TimeSpan.FromMilliseconds(100);
        private static readonly string[] Categories = { "startup", "host", "dispatcher", "callback", "bridge", "unhandled", "task", "trap" };
        private static readonly HashSet<string> StateKeys = new HashSet<string>(StringComparer.Ordinal) {
            "preview", "closed", "navigation", "pending_kind", "queue_count", "session_count", "working_count",
            "input_mode", "response_active", "bridge_running", "main_timer_enabled", "main_timer_ms",
            "response_timer_enabled", "response_timer_ms", "search_timer_enabled", "search_timer_ms", "terminating"
        };
        private static IDictionary context = new Hashtable();
        private static int installed;
        public static string BuildIdentity { get; private set; }
        public static string ScriptSha256 { get; private set; }

        public static string DirectoryPath {
            get {
                var root = Environment.GetEnvironmentVariable("LOCALAPPDATA");
                if (String.IsNullOrWhiteSpace(root)) root = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                return Path.Combine(root, "AlphaCode");
            }
        }

        public static void Initialize(Assembly assembly, string script)
        {
            var version = (AssemblyInformationalVersionAttribute)Attribute.GetCustomAttribute(assembly, typeof(AssemblyInformationalVersionAttribute));
            BuildIdentity = version == null ? "source-script (unversioned)" : version.InformationalVersion;
            using (var hash = SHA256.Create())
                ScriptSha256 = BitConverter.ToString(hash.ComputeHash(Encoding.UTF8.GetBytes(script ?? ""))).Replace("-", "").ToLowerInvariant();
        }

        public static void InstallProcessHandlers()
        {
            if (Interlocked.Exchange(ref installed, 1) != 0) return;
            AppDomain.CurrentDomain.UnhandledException += delegate(object sender, UnhandledExceptionEventArgs eventArgs) {
                Write("unhandled", "AppDomain.UnhandledException", eventArgs.ExceptionObject,
                    new Hashtable { { "terminating", eventArgs.IsTerminating } }, null);
                // This is diagnostic only; a fatal CLR exception is not recoverable.
            };
            TaskScheduler.UnobservedTaskException += delegate(object sender, UnobservedTaskExceptionEventArgs eventArgs) {
                Write("task", "TaskScheduler.UnobservedTaskException", eventArgs.Exception, null, null);
                eventArgs.SetObserved();
            };
        }

        public static bool IsFatal(Exception exception)
        {
            for (var current = exception; current != null; current = current.InnerException) {
                if (current is OutOfMemoryException || current is StackOverflowException || current is AccessViolationException || current is System.Runtime.InteropServices.SEHException)
                    return true;
            }
            var aggregate = exception as AggregateException;
            if (aggregate != null) foreach (var inner in aggregate.InnerExceptions) if (IsFatal(inner)) return true;
            return false;
        }

        public static void SetContext(IDictionary state)
        {
            var safe = SafeState(state);
            lock (Gate) { context = safe; }
        }

        private static Hashtable SafeState(IDictionary state)
        {
            var safe = new Hashtable();
            if (state == null) return safe;
            foreach (DictionaryEntry entry in state) {
                var key = entry.Key as string;
                if (key == null || !StateKeys.Contains(key)) continue;
                var value = entry.Value;
                if (value is bool || value is int || value is long || value is double) safe[key] = value;
                else if (key == "navigation" || key == "pending_kind") {
                    var text = value as string;
                    // These are internal enum/action names, never session titles,
                    // paths, prompts, search text or provider data.
                    if (text != null && Regex.IsMatch(text, "^[A-Za-z_]{0,32}$", RegexOptions.CultureInvariant, RegexTimeout)) safe[key] = text;
                }
            }
            return safe;
        }

        private static string Replace(string text, string pattern, string replacement)
        {
            return Regex.Replace(text, pattern, replacement, RegexOptions.CultureInvariant, RegexTimeout);
        }

        public static string Redact(string text, string[] knownPrivateValues)
        {
            if (String.IsNullOrEmpty(text)) return text ?? "";
            try {
                if (knownPrivateValues != null) foreach (var value in knownPrivateValues)
                    if (!String.IsNullOrWhiteSpace(value)) text = text.Replace(value, "[REDACTED]");
                foreach (var name in new[] { "USERPROFILE", "LOCALAPPDATA", "HOME" }) {
                    var path = Environment.GetEnvironmentVariable(name);
                    if (!String.IsNullOrEmpty(path) && path.Length > 3) {
                        text = Replace(text, "(?i)" + Regex.Escape(path), "<" + name + ">");
                        text = Replace(text, "(?i)" + Regex.Escape(path.Replace('\\', '/')), "<" + name + ">");
                    }
                }
                text = Replace(text, @"(?i)(\bBearer\s+)[A-Za-z0-9._~+/=-]+", "$1[REDACTED]");
                text = Replace(text, "(?i)((?:api[ _-]?key|access[ _-]?token|refresh[ _-]?token|password|secret|token|authorization|client[ _-]?secret)[\"']?\\s*[:=]\\s*)(?:\"[^\"]*\"|'[^']*'|[^\\s,;}&]+)", "$1[REDACTED]");
                text = Replace(text, @"\b(?:sk|rk|pk)-(?:proj-|or-v1-|ant-)?[A-Za-z0-9_-]{12,}\b|\bgh[pousr]_[A-Za-z0-9_]{16,}\b|\bAKIA[A-Z0-9]{16}\b", "[REDACTED]");
                text = Replace(text, @"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b", "[REDACTED]");
                text = Replace(text, @"(?i)(https?://)[^/@\s:]+:[^/@\s]+@", "$1[REDACTED]@");
                text = Replace(text, @"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "[EMAIL]");
                text = Replace(text, @"(?i)\b[A-Z]:[\\/][^\r\n\""<>|]*|\\\\[^\s\""<>|]+", "<PATH>");
                return text;
            } catch {
                // A redaction failure must never leak the unfiltered input or
                // throw from an exception handler.
                return "[diagnostic text omitted: redaction failed]";
            }
        }

        private static string Limit(string text, int limit)
        {
            return text != null && text.Length > limit ? text.Substring(0, limit) + " [truncated]" : text ?? "";
        }

        private static Dictionary<string, object> ErrorInfo(object error, string[] privateValues, int depth, ref int remaining)
        {
            var result = new Dictionary<string, object>();
            if (remaining-- <= 0) { result["message"] = "Additional exceptions omitted"; return result; }
            var record = error as ErrorRecord;
            var exception = record == null ? error as Exception : record.Exception;
            if (exception == null) {
                result["type"] = error == null ? "null" : error.GetType().FullName;
                result["message"] = "Non-exception payload omitted";
                return result;
            }
            result["type"] = exception.GetType().FullName;
            result["message"] = Limit(Redact(exception.Message, privateValues), 16384);
            result["stack"] = Limit(Redact(exception.StackTrace, privateValues), 32768);
            if (record == null) {
                var runtime = exception as RuntimeException;
                if (runtime != null) record = runtime.ErrorRecord;
            }
            if (record != null) {
                result["error_id"] = Limit(Redact(record.FullyQualifiedErrorId, privateValues), 2048);
                result["script_stack"] = Limit(Redact(record.ScriptStackTrace, privateValues), 32768);
                if (record.InvocationInfo != null) {
                    result["script_name"] = Redact(Path.GetFileName(record.InvocationInfo.ScriptName ?? ""), privateValues);
                    result["script_line"] = record.InvocationInfo.ScriptLineNumber;
                }
                // InvocationInfo.Line and TargetObject can contain credentials,
                // user text and entire request payloads. Deliberately omit them.
            }
            if (depth < 8 && remaining > 0 && exception.InnerException != null) result["inner"] = ErrorInfo(exception.InnerException, privateValues, depth + 1, ref remaining);
            var aggregate = exception as AggregateException;
            if (depth < 8 && aggregate != null) {
                var inners = new List<object>();
                foreach (var inner in aggregate.InnerExceptions) {
                    if (inners.Count == 8 || remaining <= 0) break;
                    inners.Add(ErrorInfo(inner, privateValues, depth + 1, ref remaining));
                }
                result["aggregate_inner"] = inners;
            }
            return result;
        }

        public static void Write(string category, string callback, object error, IDictionary state, string[] privateValues)
        {
            try {
                if (Array.IndexOf(Categories, category) < 0) category = "callback";
                Hashtable safe;
                lock (Gate) { safe = SafeState(context); }
                foreach (DictionaryEntry entry in SafeState(state)) safe[entry.Key] = entry.Value;
                var remaining = 32;
                var item = new Dictionary<string, object> {
                    { "timestamp_utc", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture) },
                    { "category", category }, { "callback", Limit(Redact(callback, privateValues), 256) },
                    { "build", BuildIdentity ?? "unknown" }, { "gui_sha256", ScriptSha256 ?? "unknown" },
                    { "pid", Process.GetCurrentProcess().Id }, { "thread", Thread.CurrentThread.ManagedThreadId },
                    { "apartment", Thread.CurrentThread.GetApartmentState().ToString() },
                    { "framework", Environment.Version.ToString() }, { "powershell", typeof(PSObject).Assembly.GetName().Version.ToString() },
                    { "state", safe }, { "exception", ErrorInfo(error, privateValues, 0, ref remaining) }
                };
                var json = new JavaScriptSerializer { MaxJsonLength = 2 * 1024 * 1024 }.Serialize(item);
                lock (Gate) {
                    Directory.CreateDirectory(DirectoryPath);
                    var path = Path.Combine(DirectoryPath, "gui-" + category + ".log");
                    if (File.Exists(path) && new FileInfo(path).Length >= MaxLogBytes) {
                        var previous = path + ".1";
                        if (File.Exists(previous)) File.Delete(previous);
                        File.Move(path, previous);
                    }
                    File.AppendAllText(path, json + Environment.NewLine, new UTF8Encoding(false));
                }
            } catch {
                // Diagnostic I/O must not create a second Dispatcher exception.
                try { Console.Error.WriteLine("AlphaCode GUI diagnostic could not be written (details omitted)."); } catch { }
            }
        }

        public static string Export(string destination)
        {
            var target = Path.GetFullPath(destination);
            if (String.Equals(target.TrimEnd(Path.DirectorySeparatorChar), DirectoryPath.TrimEnd(Path.DirectorySeparatorChar), StringComparison.OrdinalIgnoreCase))
                throw new ArgumentException("Choose a separate diagnostic export directory.");
            Directory.CreateDirectory(target);
            foreach (var category in Categories) foreach (var suffix in new[] { "", ".1" }) {
                var name = "gui-" + category + ".log" + suffix;
                var source = Path.Combine(DirectoryPath, name);
                if (File.Exists(source)) {
                    // Redact string values BEFORE serializing. Running regexes
                    // over JSON itself can remove quotes and corrupt records.
                    var serializer = new JavaScriptSerializer { MaxJsonLength = 2 * 1024 * 1024 };
                    var exported = new StringBuilder();
                    foreach (var line in File.ReadLines(source)) {
                        try { exported.AppendLine(serializer.Serialize(RedactValue(serializer.DeserializeObject(line)))); }
                        catch { exported.AppendLine(serializer.Serialize(new Hashtable { { "category", category }, { "message", "Legacy/unparseable diagnostic omitted" } })); }
                    }
                    File.WriteAllText(Path.Combine(target, name), exported.ToString(), new UTF8Encoding(false));
                }
            }
            var manifest = new Dictionary<string, object> {
                { "collected_utc", DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture) },
                { "build", BuildIdentity ?? "unknown" }, { "gui_sha256", ScriptSha256 ?? "unknown" },
                { "framework", Environment.Version.ToString() }, { "configuration_and_user_payloads", "excluded" }
            };
            File.WriteAllText(Path.Combine(target, "build-info.json"), new JavaScriptSerializer().Serialize(manifest), new UTF8Encoding(false));
            return target;
        }

        private static object RedactValue(object value)
        {
            var text = value as string;
            if (text != null) return Redact(text, null);
            var map = value as IDictionary;
            if (map != null) {
                var safe = new Dictionary<string, object>();
                foreach (DictionaryEntry item in map) safe[Redact((string)item.Key, null)] = RedactValue(item.Value);
                return safe;
            }
            var array = value as object[];
            if (array != null) {
                var safe = new object[array.Length];
                for (var i = 0; i < array.Length; i++) safe[i] = RedactValue(array[i]);
                return safe;
            }
            return value;
        }
    }
}
