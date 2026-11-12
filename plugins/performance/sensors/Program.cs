using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using LibreHardwareMonitor.Hardware;
using Microsoft.Win32;
using Microsoft.Win32.SafeHandles;

internal static class Program
{
    private const string DriverHash = "1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032";
    private static readonly JsonSerializerOptions Json = new() { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower };
    private static readonly object OutputLock = new();
    private static int parentPid;
    private static CancellationTokenSource stopping = new();
    private static Process? parent;

    public static async Task<int> Main(string[] args)
    {
        if (args.SequenceEqual(new[] { "--self-test" })) return await SelfTest();
        if (args.Length < 2 || args[0] != "--parent" || !int.TryParse(args[1], out parentPid)) return 2;
        try
        {
            parent = Process.GetProcessById(parentPid);
            // Keep the original process handle open: a reused PID cannot extend this lease.
            _ = parent.SafeHandle;
            _ = WatchParent();
            if (args.Length == 4 && args[2] == "--broker") return await Broker(args[3]);
            if (args.Length == 4 && args[2] == "--fixture") return await PipeFixture(args[3]);
            if (args.Length != 2 && !(args.Length == 3 && args[2] == "--authorize")) return 2;
            _ = Task.Run(() => ReadCommands(Console.In, false));
            if (args.Length == 3) return await Authorized();
            await Collect(Console.Out, stopping.Token);
            return 0;
        }
        catch (OperationCanceledException) { return 0; }
        catch (Exception e)
        {
            Emit(Console.Out, new { sources = Array.Empty<object>(), data = Array.Empty<object>(), hardware_status = "error", message = Clean(e.Message, 512) });
            return 3;
        }
        finally { parent?.Dispose(); }
    }

    private static async Task WatchParent()
    {
        try { await parent!.WaitForExitAsync(stopping.Token); stopping.Cancel(); }
        catch (OperationCanceledException) { }
        // A hung hardware call must not leave an orphaned elevated collector.
        try { await Task.Delay(4500); if (stopping.IsCancellationRequested) Environment.Exit(4); }
        catch { Environment.Exit(4); }
    }

    private static string Clean(string value, int limit) => string.Concat(value.Where(c => !char.IsControl(c)).Take(limit));
    private static void Emit(TextWriter writer, object value)
    {
        lock (OutputLock) { writer.WriteLine(JsonSerializer.Serialize(value, Json)); writer.Flush(); }
    }
    private static void Status(string state, string message) => Emit(Console.Out, new { sources = Array.Empty<object>(), data = Array.Empty<object>(), hardware_status = state, message });
    private static async Task<string?> BoundedLine(TextReader reader, int limit, CancellationToken token)
    {
        var value = new StringBuilder(); var buffer = new char[1];
        while (await reader.ReadAsync(buffer.AsMemory(), token) != 0)
        {
            if (buffer[0] == '\n') return value.ToString().TrimEnd('\r');
            if (value.Length >= limit) throw new IOException("采集消息超过限制");
            value.Append(buffer[0]);
        }
        return null;
    }
    private static async Task ReadCommands(TextReader reader, bool heartbeat)
    {
        long last = Stopwatch.GetTimestamp();
        if (heartbeat) _ = Task.Run(async () =>
        {
            while (!stopping.IsCancellationRequested)
            {
                await Task.Delay(500);
                if (Stopwatch.GetElapsedTime(Interlocked.Read(ref last)).TotalSeconds > 3) { stopping.Cancel(); break; }
            }
            await Task.Delay(1000); Environment.Exit(4);
        });
        try
        {
            while (!stopping.IsCancellationRequested)
            {
                var command = await BoundedLine(reader, 128, stopping.Token);
                if (command is null or "stop") break;
                if (command is not ("ping" or "enumerate" or "subscribe" or "read")) break;
                Interlocked.Exchange(ref last, Stopwatch.GetTimestamp());
            }
        }
        catch (OperationCanceledException) { }
        catch (IOException) { }
        finally { stopping.Cancel(); }
    }

    private static bool DriverInstalled()
    {
        using var root = RegistryKey.OpenBaseKey(RegistryHive.LocalMachine, RegistryView.Registry64);
        using var key = root.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO");
        return Version.TryParse(key?.GetValue("DisplayVersion") as string, out var version) && version >= new Version(2, 2, 0);
    }
    private static bool DriverAccessible()
    {
        using var handle = CreateFileW(@"\\?\GLOBALROOT\Device\PawnIO", 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
        return !handle.IsInvalid;
    }

    private static async Task<int> Authorized()
    {
        Status("starting", "正在准备硬件传感器，请完成系统授权…");
        if (!DriverInstalled())
        {
            var setup = Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "driver", "PawnIO_setup.exe"));
            if (!File.Exists(setup) || !Convert.ToHexString(SHA256.HashData(await File.ReadAllBytesAsync(setup, stopping.Token))).Equals(DriverHash, StringComparison.OrdinalIgnoreCase))
                throw new IOException("传感器驱动资源校验失败，请重新安装客户端");
            try
            {
                using var installer = await Task.Run(() => Process.Start(new ProcessStartInfo(setup) { UseShellExecute = true, Verb = "runas" }), stopping.Token);
                if (installer is null) throw new IOException("无法启动传感器驱动安装程序");
                await installer.WaitForExitAsync(stopping.Token);
                if (installer.ExitCode != 0 || !DriverInstalled()) { Status("driver_required", "驱动尚未安装，基础性能采集继续运行"); await Idle(); return 0; }
            }
            catch (Win32Exception e) when (e.NativeErrorCode == 1223) { Status("driver_required", "已取消驱动安装，基础性能采集继续运行"); await Idle(); return 0; }
        }
        if (DriverAccessible()) { await Collect(Console.Out, stopping.Token); return 0; }
        var pipeName = "M18.Sensors." + Guid.NewGuid().ToString("N");
        var security = new PipeSecurity();
        security.SetAccessRuleProtection(true, false);
        security.AddAccessRule(new PipeAccessRule(WindowsIdentity.GetCurrent().User!, PipeAccessRights.FullControl, AccessControlType.Allow));
        await using var pipe = NamedPipeServerStreamAcl.Create(pipeName, PipeDirection.InOut, 1, PipeTransmissionMode.Byte, PipeOptions.Asynchronous, 65536, 65536, security);
        Process? broker = null;
        try
        {
            var executable = Environment.ProcessPath ?? throw new IOException("采集程序路径不可用");
            var start = new ProcessStartInfo(executable) { UseShellExecute = true, Verb = "runas", WindowStyle = ProcessWindowStyle.Hidden, WorkingDirectory = AppContext.BaseDirectory };
            start.ArgumentList.Add("--parent"); start.ArgumentList.Add(Environment.ProcessId.ToString(CultureInfo.InvariantCulture)); start.ArgumentList.Add("--broker"); start.ArgumentList.Add(pipeName);
            try { broker = await Task.Run(() => Process.Start(start), stopping.Token); }
            catch (Win32Exception e) when (e.NativeErrorCode == 1223) { Status("permission_required", "已取消系统授权，可点击重试；基础性能采集继续运行"); await Idle(); return 0; }
            if (broker is null) throw new IOException("无法启动硬件采集代理");
            using var connection = CancellationTokenSource.CreateLinkedTokenSource(stopping.Token);
            connection.CancelAfter(TimeSpan.FromSeconds(120));
            await pipe.WaitForConnectionAsync(connection.Token);
            if (!GetNamedPipeClientProcessId(pipe.SafePipeHandle, out var client) || client != broker.Id) throw new IOException("硬件采集代理身份不匹配");
            ValidatePeer(broker);
            using var writer = new PipeWriter(pipe) { AutoFlush = true };
            using var reader = new StreamReader(pipe, Encoding.UTF8, leaveOpen: true);
            var heartbeat = Task.Run(async () =>
            {
                try { while (!stopping.IsCancellationRequested) { await writer.WriteLineAsync("ping"); await Task.Delay(500, stopping.Token); } }
                catch (Exception e) when (e is IOException or OperationCanceledException or ObjectDisposedException) { }
            });
            try
            {
                while (!stopping.IsCancellationRequested)
                {
                    var line = await BoundedLine(reader, 1_048_576, stopping.Token);
                    if (line is null) { if (!stopping.IsCancellationRequested) throw new IOException("硬件采集代理已退出"); break; }
                    // No commands or code flow in the privileged-to-unprivileged direction.
                    using var parsed = JsonDocument.Parse(line);
                    Emit(Console.Out, parsed.RootElement);
                }
            }
            finally { stopping.Cancel(); await heartbeat; }
        }
        finally
        {
            stopping.Cancel();
            pipe.DisconnectIfConnected();
            if (broker is not null)
            {
                using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(4));
                try { await broker.WaitForExitAsync(deadline.Token); }
                catch (OperationCanceledException) { Status("error", "提权采集代理停止超时"); }
                broker.Dispose();
            }
        }
        return 0;
    }

    private sealed class PipeWriter(Stream stream) : StreamWriter(stream, new UTF8Encoding(false), leaveOpen:true)
    {
        protected override void Dispose(bool disposing)
        {
            try { base.Dispose(disposing); }
            catch (Exception e) when (e is IOException or InvalidOperationException) { }
        }
    }
    private static async Task Idle() { try { await Task.Delay(Timeout.Infinite, stopping.Token); } catch (OperationCanceledException) { } }
    private static void DisconnectIfConnected(this NamedPipeServerStream pipe) { if (pipe.IsConnected) pipe.Disconnect(); }
    private static async Task<int> Broker(string name)
    {
        if (!name.StartsWith("M18.Sensors.", StringComparison.Ordinal) || name.Length != 44 || !Guid.TryParseExact(name[12..], "N", out _)) return 2;
        ValidatePeer(parent!);
        using var job = OwnJob();
        using var pipe = new NamedPipeClientStream(".", name, PipeDirection.InOut, PipeOptions.Asynchronous);
        await pipe.ConnectAsync(10000, stopping.Token);
        if (!GetNamedPipeServerProcessId(pipe.SafePipeHandle, out var server) || server != parentPid) return 2;
        using var reader = new StreamReader(pipe, Encoding.UTF8, leaveOpen: true);
        using var writer = new PipeWriter(pipe) { AutoFlush = true };
        _ = ReadCommands(reader, true);
        await Collect(writer, stopping.Token);
        return 0;
    }
    private static void ValidatePeer(Process peer)
    {
        using var handle=OpenProcess(0x1000, false, (uint)peer.Id);
        var path=new StringBuilder(32768); uint size=(uint)path.Capacity;
        if (handle.IsInvalid || !QueryFullProcessImageNameW(handle,0,path,ref size) || peer.SessionId != Process.GetCurrentProcess().SessionId || !string.Equals(path.ToString(), Environment.ProcessPath, StringComparison.OrdinalIgnoreCase)) throw new IOException("采集会话或程序身份不匹配");
    }

    private static async Task Collect(TextWriter output, CancellationToken token)
    {
        bool installed = DriverInstalled(), accessible = installed && DriverAccessible();
        var computer = new Computer { IsCpuEnabled = accessible, IsMotherboardEnabled = accessible, IsGpuEnabled = true, IsStorageEnabled = true, IsControllerEnabled = true };
        // Keep blocking device probing outside the pipe/input task and cancellation watchdog.
        try { await Task.Run(async () =>
        {
            computer.Open();
            while (!token.IsCancellationRequested)
            {
                var sources = new List<object>(); var data = new List<object>(); var failures = new List<string>();
                foreach (var hardware in computer.Hardware) Visit(hardware, "", sources, data, failures);
                Emit(output, new { sources, data, hardware_status = !installed ? "driver_required" : !accessible ? "permission_required" : failures.Count > 0 ? "error" : "ready", message = !installed ? "CPU、主板等传感器需要安装驱动" : !accessible ? "部分硬件传感器需要系统授权" : failures.Count > 0 ? Clean(string.Join("；", failures.Distinct()), 512) : null });
                await Task.Delay(1000, token);
            }
        }, token); } finally { computer.Close(); }
    }
    private static void Visit(IHardware hardware, string path, List<object> sources, List<object> data, List<string> failures)
    {
        try { hardware.Update(); }
        catch (Exception e) { failures.Add(Clean(hardware.Name + "：" + e.Message, 160)); return; }
        path = string.IsNullOrEmpty(path) ? hardware.Name : path + " / " + hardware.Name;
        foreach (var sensor in hardware.Sensors)
        {
            var kind = sensor.SensorType switch { SensorType.Temperature => "temperature", SensorType.Fan => "fan", SensorType.Power => "power", SensorType.Voltage => "voltage", _ => null };
            if (kind is null || sources.Count >= 256) continue;
            var id = kind + "-" + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(sensor.Identifier.ToString()))).ToLowerInvariant();
            bool valid = sensor.Value.HasValue && float.IsFinite(sensor.Value.Value);
            sources.Add(new { widget_id = kind, id, name = Clean(path + " / " + sensor.Name, 160), available = valid, error = valid ? null : "探头暂无数据" });
            data.Add(new { widget_id = kind, source_id = id, title = Clean(sensor.Name, 32), value = valid ? Format(kind, sensor.Value!.Value) : "—", secondary_value = (string?)null, detail = kind switch {"fan"=>"RPM", "power"=>"W", "voltage"=>"V", _=>(string?)null}, progress = (double?)null, valid, presentations = new Dictionary<string, object>() });
        }
        foreach (var sub in hardware.SubHardware) Visit(sub, path, sources, data, failures);
    }
    private static string Format(string kind, float value) => kind switch
    {
        "temperature" => value.ToString("0.0", CultureInfo.InvariantCulture) + "°C",
        "fan" => value.ToString("0", CultureInfo.InvariantCulture),
        "power" => value.ToString("0.0", CultureInfo.InvariantCulture),
        _ => value.ToString("0.00", CultureInfo.InvariantCulture)
    };

    private static SafeFileHandle OwnJob()
    {
        var job = CreateJobObjectW(IntPtr.Zero, null);
        if (job.IsInvalid) throw new Win32Exception();
        var info = new JobLimits { Basic = new BasicLimits { Flags = 0x2000 } };
        if (!SetInformationJobObject(job, 9, ref info, (uint)Marshal.SizeOf<JobLimits>()) || !AssignProcessToJobObject(job, Process.GetCurrentProcess().Handle)) { job.Dispose(); throw new Win32Exception(); }
        return job;
    }
    private static async Task<int> PipeFixture(string name)
    {
        if (!name.StartsWith("M18.Sensors.", StringComparison.Ordinal) || !Guid.TryParseExact(name[12..], "N", out _)) return 2;
        ValidatePeer(parent!);
        using var pipe = new NamedPipeClientStream(".", name, PipeDirection.InOut, PipeOptions.Asynchronous);
        await pipe.ConnectAsync(5000, stopping.Token);
        if (!GetNamedPipeServerProcessId(pipe.SafePipeHandle, out var server) || server != parentPid) return 2;
        using var reader = new StreamReader(pipe, Encoding.UTF8, leaveOpen:true);
        using var writer = new PipeWriter(pipe) { AutoFlush = true };
        Emit(writer, new {sources=Array.Empty<object>(),data=Array.Empty<object>(),hardware_status="ready",message=(string?)null});
        await ReadCommands(reader,true);
        return 0;
    }
    private static async Task<int> SelfTest()
    {
        if (Format("temperature", 42.3f) != "42.3°C" || Format("fan", 0) != "0" || Format("voltage", 1.25f) != "1.25") return 1;
        if (Clean("a\nb", 2) != "ab") return 1;
        using (var own=Process.GetCurrentProcess()) ValidatePeer(own);
        try { await BoundedLine(new StringReader(new string('x',129)),128,CancellationToken.None); return 1; } catch(IOException) { }
        foreach(var mode in new[]{"stop","disconnect","timeout"})
        {
            var name="M18.Sensors."+Guid.NewGuid().ToString("N");
            using var pipe=new NamedPipeServerStream(name,PipeDirection.InOut,1,PipeTransmissionMode.Byte,PipeOptions.Asynchronous|PipeOptions.CurrentUserOnly);
            var info=new ProcessStartInfo(Environment.ProcessPath!) {UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true,RedirectStandardError=true};
            info.ArgumentList.Add("--parent");info.ArgumentList.Add(Environment.ProcessId.ToString(CultureInfo.InvariantCulture));info.ArgumentList.Add("--fixture");info.ArgumentList.Add(name);
            using var child=Process.Start(info)!;
            using var deadline=new CancellationTokenSource(TimeSpan.FromSeconds(8));
            await pipe.WaitForConnectionAsync(deadline.Token);
            if(!GetNamedPipeClientProcessId(pipe.SafePipeHandle,out var client)||client!=child.Id) return 1;
            ValidatePeer(child);
            using var reader=new StreamReader(pipe,Encoding.UTF8,leaveOpen:true);
            using var writer=new PipeWriter(pipe){AutoFlush=true};
            using var sample=JsonDocument.Parse((await BoundedLine(reader,1024,deadline.Token))!);
            if(sample.RootElement.GetProperty("hardware_status").GetString()!="ready")return 1;
            await writer.WriteLineAsync("ping");
            if(mode=="stop")await writer.WriteLineAsync("stop");
            if(mode=="disconnect") { writer.Dispose(); pipe.Disconnect(); }
            await child.WaitForExitAsync(deadline.Token);
            if(mode!="timeout" && child.ExitCode!=0)return 1;
            if(mode=="timeout" && child.ExitCode is not (0 or 4))return 1;
        }
        Console.WriteLine("Sensor formats, bounded input, peer identity, pipe stop/disconnect/heartbeat timeout passed; no hardware or UAC opened."); return 0;
    }
    [StructLayout(LayoutKind.Sequential)] private struct BasicLimits { public long ProcessTime, JobTime; public uint Flags; public nuint Minimum, Maximum; public uint Active; public nuint Affinity; public uint Priority, Scheduling; }
    [StructLayout(LayoutKind.Sequential)] private struct IoCounters { public ulong Read, Write, Other, ReadBytes, WriteBytes, OtherBytes; }
    [StructLayout(LayoutKind.Sequential)] private struct JobLimits { public BasicLimits Basic; public IoCounters Io; public nuint ProcessMemory, JobMemory, PeakProcess, PeakJob; }
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] private static extern SafeFileHandle CreateFileW(string name, uint access, uint sharing, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool GetNamedPipeClientProcessId(SafePipeHandle pipe, out uint pid);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool GetNamedPipeServerProcessId(SafePipeHandle pipe, out uint pid);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] private static extern SafeFileHandle CreateJobObjectW(IntPtr security, string? name);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool SetInformationJobObject(SafeFileHandle job, int kind, ref JobLimits info, uint length);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool AssignProcessToJobObject(SafeFileHandle job, IntPtr process);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern SafeFileHandle OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] private static extern bool QueryFullProcessImageNameW(SafeFileHandle process, uint flags, StringBuilder path, ref uint size);
}
