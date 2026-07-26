using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace Verbatim.NativeSmoke
{
    public sealed class WasapiPlaybackReceipt
    {
        public bool checked_device;
        public bool device_found;
        public bool stream_started;
        public bool success;
        public string device_name;
        public int source_sample_rate;
        public int source_channels;
        public long source_frames;
        public long submitted_frames;
        public string failure_class;
    }

    public static class WasapiWavPlayback
    {
        private const uint DeviceStateActive = 0x00000001;
        private const uint ClsCtxAll = 23;
        private const ushort WaveFormatPcm = 1;
        private const int PropVariantLpWStr = 31;
        private const int PropVariantBStr = 8;

        public static string[] ListActiveRenderDevices()
        {
            return ListActiveDevices(EDataFlow.Render);
        }

        public static string[] ListActiveCaptureDevices()
        {
            return ListActiveDevices(EDataFlow.Capture);
        }

        private static string[] ListActiveDevices(EDataFlow dataFlow)
        {
            var names = new List<string>();
            IMMDeviceEnumerator enumerator = null;
            IMMDeviceCollection collection = null;
            try
            {
                enumerator = (IMMDeviceEnumerator)new MMDeviceEnumeratorComObject();
                ThrowIfFailed(
                    enumerator.EnumAudioEndpoints(dataFlow, DeviceStateActive, out collection),
                    "enumerate audio devices");
                uint count;
                ThrowIfFailed(collection.GetCount(out count), "count render devices");
                for (uint index = 0; index < count; index++)
                {
                    IMMDevice device = null;
                    try
                    {
                        ThrowIfFailed(collection.Item(index, out device), "read render device");
                        names.Add(GetFriendlyName(device));
                    }
                    finally
                    {
                        ReleaseCom(device);
                    }
                }
            }
            finally
            {
                ReleaseCom(collection);
                ReleaseCom(enumerator);
            }
            return names.ToArray();
        }

        public static WasapiPlaybackReceipt PlayMonoPcm16(string wavPath, string deviceName)
        {
            var receipt = new WasapiPlaybackReceipt
            {
                checked_device = true,
                device_found = false,
                stream_started = false,
                success = false,
                device_name = deviceName,
                source_sample_rate = 0,
                source_channels = 0,
                source_frames = 0,
                submitted_frames = 0,
                failure_class = null,
            };

            WavData wav;
            try
            {
                wav = ReadMonoPcm16(wavPath);
            }
            catch (FileNotFoundException)
            {
                receipt.failure_class = "wav_not_found";
                return receipt;
            }
            catch (InvalidDataException)
            {
                receipt.failure_class = "invalid_wav";
                return receipt;
            }
            catch
            {
                receipt.failure_class = "wav_read_failed";
                return receipt;
            }

            receipt.source_sample_rate = wav.SampleRate;
            receipt.source_channels = wav.Channels;
            receipt.source_frames = wav.Data.Length / wav.BlockAlign;

            IMMDeviceEnumerator enumerator = null;
            IMMDeviceCollection collection = null;
            IMMDevice selectedDevice = null;
            IAudioClient audioClient = null;
            IAudioRenderClient renderClient = null;
            IntPtr formatPointer = IntPtr.Zero;
            try
            {
                enumerator = (IMMDeviceEnumerator)new MMDeviceEnumeratorComObject();
                ThrowIfFailed(
                    enumerator.EnumAudioEndpoints(EDataFlow.Render, DeviceStateActive, out collection),
                    "enumerate render devices");
                uint count;
                ThrowIfFailed(collection.GetCount(out count), "count render devices");
                for (uint index = 0; index < count; index++)
                {
                    IMMDevice candidate = null;
                    ThrowIfFailed(collection.Item(index, out candidate), "read render device");
                    var candidateName = GetFriendlyName(candidate);
                    if (String.Equals(candidateName, deviceName, StringComparison.OrdinalIgnoreCase))
                    {
                        selectedDevice = candidate;
                        receipt.device_found = true;
                        break;
                    }
                    ReleaseCom(candidate);
                }

                if (selectedDevice == null)
                {
                    receipt.failure_class = "render_device_not_found";
                    return receipt;
                }

                object audioClientObject;
                var audioClientId = typeof(IAudioClient).GUID;
                ThrowIfFailed(
                    selectedDevice.Activate(ref audioClientId, ClsCtxAll, IntPtr.Zero, out audioClientObject),
                    "activate render device");
                audioClient = (IAudioClient)audioClientObject;

                var format = new WaveFormatEx
                {
                    wFormatTag = WaveFormatPcm,
                    nChannels = (ushort)wav.Channels,
                    nSamplesPerSec = (uint)wav.SampleRate,
                    nAvgBytesPerSec = (uint)(wav.SampleRate * wav.BlockAlign),
                    nBlockAlign = (ushort)wav.BlockAlign,
                    wBitsPerSample = 16,
                    cbSize = 0,
                };
                formatPointer = Marshal.AllocCoTaskMem(Marshal.SizeOf(typeof(WaveFormatEx)));
                Marshal.StructureToPtr(format, formatPointer, false);

                var sessionId = Guid.Empty;
                ThrowIfFailed(
                    audioClient.Initialize(
                        AudioClientShareMode.Shared,
                        AudioClientStreamFlags.AutoConvertPcm | AudioClientStreamFlags.SrcDefaultQuality,
                        0,
                        0,
                        formatPointer,
                        ref sessionId),
                    "initialize shared render client");

                uint bufferFrames;
                ThrowIfFailed(audioClient.GetBufferSize(out bufferFrames), "read render buffer size");
                object renderClientObject;
                var renderClientId = typeof(IAudioRenderClient).GUID;
                ThrowIfFailed(audioClient.GetService(ref renderClientId, out renderClientObject), "get render service");
                renderClient = (IAudioRenderClient)renderClientObject;

                ThrowIfFailed(audioClient.Start(), "start render stream");
                receipt.stream_started = true;
                SubmitAllFrames(audioClient, renderClient, bufferFrames, wav, receipt);
                Drain(audioClient);
                ThrowIfFailed(audioClient.Stop(), "stop render stream");
                receipt.success = receipt.submitted_frames == receipt.source_frames;
                if (!receipt.success)
                {
                    receipt.failure_class = "incomplete_render";
                }
            }
            catch (COMException)
            {
                receipt.failure_class = receipt.stream_started ? "render_failed" : "render_initialization_failed";
            }
            catch
            {
                receipt.failure_class = receipt.stream_started ? "render_failed" : "render_initialization_failed";
            }
            finally
            {
                if (audioClient != null)
                {
                    audioClient.Stop();
                }
                if (formatPointer != IntPtr.Zero)
                {
                    Marshal.FreeCoTaskMem(formatPointer);
                }
                ReleaseCom(renderClient);
                ReleaseCom(audioClient);
                ReleaseCom(selectedDevice);
                ReleaseCom(collection);
                ReleaseCom(enumerator);
            }

            return receipt;
        }

        private static void SubmitAllFrames(
            IAudioClient audioClient,
            IAudioRenderClient renderClient,
            uint bufferFrames,
            WavData wav,
            WasapiPlaybackReceipt receipt)
        {
            var byteOffset = 0;
            while (byteOffset < wav.Data.Length)
            {
                uint paddingFrames;
                ThrowIfFailed(audioClient.GetCurrentPadding(out paddingFrames), "read render padding");
                var availableFrames = bufferFrames - paddingFrames;
                if (availableFrames == 0)
                {
                    Thread.Sleep(5);
                    continue;
                }

                var remainingFrames = (uint)((wav.Data.Length - byteOffset) / wav.BlockAlign);
                var framesToWrite = Math.Min(availableFrames, remainingFrames);
                IntPtr buffer;
                ThrowIfFailed(renderClient.GetBuffer(framesToWrite, out buffer), "acquire render buffer");
                try
                {
                    Marshal.Copy(wav.Data, byteOffset, buffer, checked((int)(framesToWrite * wav.BlockAlign)));
                }
                finally
                {
                    ThrowIfFailed(renderClient.ReleaseBuffer(framesToWrite, 0), "release render buffer");
                }
                byteOffset += checked((int)(framesToWrite * wav.BlockAlign));
                receipt.submitted_frames += framesToWrite;
            }
        }

        private static void Drain(IAudioClient audioClient)
        {
            var deadline = DateTime.UtcNow.AddSeconds(5);
            while (DateTime.UtcNow < deadline)
            {
                uint paddingFrames;
                ThrowIfFailed(audioClient.GetCurrentPadding(out paddingFrames), "drain render buffer");
                if (paddingFrames == 0)
                {
                    return;
                }
                Thread.Sleep(10);
            }
        }

        private static WavData ReadMonoPcm16(string path)
        {
            if (!File.Exists(path))
            {
                throw new FileNotFoundException("WAV fixture does not exist", path);
            }

            using (var stream = File.Open(path, FileMode.Open, FileAccess.Read, FileShare.Read))
            using (var reader = new BinaryReader(stream, Encoding.ASCII))
            {
                if (ReadChunkId(reader) != "RIFF")
                {
                    throw new InvalidDataException("missing RIFF header");
                }
                reader.ReadUInt32();
                if (ReadChunkId(reader) != "WAVE")
                {
                    throw new InvalidDataException("missing WAVE header");
                }

                ushort formatTag = 0;
                ushort channels = 0;
                uint sampleRate = 0;
                ushort bitsPerSample = 0;
                byte[] pcm = null;

                while (stream.Position + 8 <= stream.Length)
                {
                    var chunkId = ReadChunkId(reader);
                    var chunkSize = reader.ReadUInt32();
                    var chunkStart = stream.Position;
                    if (chunkStart + chunkSize > stream.Length)
                    {
                        throw new InvalidDataException("truncated WAV chunk");
                    }

                    if (chunkId == "fmt ")
                    {
                        if (chunkSize < 16)
                        {
                            throw new InvalidDataException("invalid fmt chunk");
                        }
                        formatTag = reader.ReadUInt16();
                        channels = reader.ReadUInt16();
                        sampleRate = reader.ReadUInt32();
                        reader.ReadUInt32();
                        reader.ReadUInt16();
                        bitsPerSample = reader.ReadUInt16();
                    }
                    else if (chunkId == "data")
                    {
                        pcm = reader.ReadBytes(checked((int)chunkSize));
                    }

                    stream.Position = chunkStart + chunkSize + (chunkSize % 2);
                }

                if (formatTag != WaveFormatPcm || channels != 1 || sampleRate != 16000 || bitsPerSample != 16 || pcm == null || pcm.Length == 0 || pcm.Length % 2 != 0)
                {
                    throw new InvalidDataException("WAV must be 16 kHz mono signed 16-bit PCM");
                }

                return new WavData
                {
                    SampleRate = checked((int)sampleRate),
                    Channels = channels,
                    BlockAlign = 2,
                    Data = pcm,
                };
            }
        }

        private static string ReadChunkId(BinaryReader reader)
        {
            var bytes = reader.ReadBytes(4);
            if (bytes.Length != 4)
            {
                throw new InvalidDataException("truncated WAV header");
            }
            return Encoding.ASCII.GetString(bytes);
        }

        private static string GetFriendlyName(IMMDevice device)
        {
            IPropertyStore store = null;
            try
            {
                ThrowIfFailed(device.OpenPropertyStore(0, out store), "open device property store");
                var key = new PropertyKey
                {
                    FormatId = new Guid("a45c254e-df1c-4efd-8020-67d146a850e0"),
                    PropertyId = 14,
                };
                PropVariant value;
                ThrowIfFailed(store.GetValue(ref key, out value), "read device friendly name");
                try
                {
                    if (value.VariantType == PropVariantLpWStr)
                    {
                        return Marshal.PtrToStringUni(value.PointerValue) ?? String.Empty;
                    }
                    if (value.VariantType == PropVariantBStr)
                    {
                        return Marshal.PtrToStringBSTR(value.PointerValue) ?? String.Empty;
                    }
                    return String.Empty;
                }
                finally
                {
                    PropVariantClear(ref value);
                }
            }
            finally
            {
                ReleaseCom(store);
            }
        }

        private static void ThrowIfFailed(int hresult, string operation)
        {
            if (hresult < 0)
            {
                Marshal.ThrowExceptionForHR(hresult);
            }
        }

        private static void ReleaseCom(object value)
        {
            if (value != null && Marshal.IsComObject(value))
            {
                Marshal.ReleaseComObject(value);
            }
        }

        [DllImport("ole32.dll")]
        private static extern int PropVariantClear(ref PropVariant value);

        private sealed class WavData
        {
            public int SampleRate;
            public int Channels;
            public int BlockAlign;
            public byte[] Data;
        }

        private enum EDataFlow
        {
            Render = 0,
            Capture = 1,
            All = 2,
        }

        private enum AudioClientShareMode
        {
            Shared = 0,
            Exclusive = 1,
        }

        [Flags]
        private enum AudioClientStreamFlags : uint
        {
            None = 0,
            AutoConvertPcm = 0x80000000,
            SrcDefaultQuality = 0x08000000,
        }

        [StructLayout(LayoutKind.Sequential, Pack = 2)]
        private struct WaveFormatEx
        {
            public ushort wFormatTag;
            public ushort nChannels;
            public uint nSamplesPerSec;
            public uint nAvgBytesPerSec;
            public ushort nBlockAlign;
            public ushort wBitsPerSample;
            public ushort cbSize;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct PropertyKey
        {
            public Guid FormatId;
            public uint PropertyId;
        }

        [StructLayout(LayoutKind.Explicit)]
        private struct PropVariant
        {
            [FieldOffset(0)]
            public ushort VariantType;

            [FieldOffset(8)]
            public IntPtr PointerValue;
        }

        [ComImport]
        [Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
        private class MMDeviceEnumeratorComObject
        {
        }

        [ComImport]
        [Guid("A95664D2-9614-4F35-A746-DE8DB63617E6")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IMMDeviceEnumerator
        {
            [PreserveSig]
            int EnumAudioEndpoints(EDataFlow dataFlow, uint stateMask, out IMMDeviceCollection devices);

            [PreserveSig]
            int GetDefaultAudioEndpoint(EDataFlow dataFlow, int role, out IMMDevice endpoint);

            [PreserveSig]
            int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string id, out IMMDevice device);

            [PreserveSig]
            int RegisterEndpointNotificationCallback(IntPtr client);

            [PreserveSig]
            int UnregisterEndpointNotificationCallback(IntPtr client);
        }

        [ComImport]
        [Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IMMDeviceCollection
        {
            [PreserveSig]
            int GetCount(out uint count);

            [PreserveSig]
            int Item(uint index, out IMMDevice device);
        }

        [ComImport]
        [Guid("D666063F-1587-4E43-81F1-B948E807363F")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IMMDevice
        {
            [PreserveSig]
            int Activate(ref Guid interfaceId, uint classContext, IntPtr activationParameters, [MarshalAs(UnmanagedType.IUnknown)] out object interfacePointer);

            [PreserveSig]
            int OpenPropertyStore(int storageMode, out IPropertyStore properties);

            [PreserveSig]
            int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);

            [PreserveSig]
            int GetState(out uint state);
        }

        [ComImport]
        [Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IPropertyStore
        {
            [PreserveSig]
            int GetCount(out uint count);

            [PreserveSig]
            int GetAt(uint index, out PropertyKey key);

            [PreserveSig]
            int GetValue(ref PropertyKey key, out PropVariant value);

            [PreserveSig]
            int SetValue(ref PropertyKey key, ref PropVariant value);

            [PreserveSig]
            int Commit();
        }

        [ComImport]
        [Guid("1CB9AD4C-DBFA-4C32-B178-C2F568A703B2")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IAudioClient
        {
            [PreserveSig]
            int Initialize(AudioClientShareMode shareMode, AudioClientStreamFlags streamFlags, long bufferDuration, long periodicity, IntPtr format, ref Guid sessionId);

            [PreserveSig]
            int GetBufferSize(out uint bufferFrames);

            [PreserveSig]
            int GetStreamLatency(out long latency);

            [PreserveSig]
            int GetCurrentPadding(out uint paddingFrames);

            [PreserveSig]
            int IsFormatSupported(AudioClientShareMode shareMode, IntPtr format, out IntPtr closestMatch);

            [PreserveSig]
            int GetMixFormat(out IntPtr format);

            [PreserveSig]
            int GetDevicePeriod(out long defaultPeriod, out long minimumPeriod);

            [PreserveSig]
            int Start();

            [PreserveSig]
            int Stop();

            [PreserveSig]
            int Reset();

            [PreserveSig]
            int SetEventHandle(IntPtr eventHandle);

            [PreserveSig]
            int GetService(ref Guid interfaceId, [MarshalAs(UnmanagedType.IUnknown)] out object service);
        }

        [ComImport]
        [Guid("F294ACFC-3146-4483-A7BF-ADDCA7C260E2")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IAudioRenderClient
        {
            [PreserveSig]
            int GetBuffer(uint framesRequested, out IntPtr data);

            [PreserveSig]
            int ReleaseBuffer(uint framesWritten, uint flags);
        }
    }
}
