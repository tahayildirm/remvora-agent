// ScreenCaptureKit system output only. Raw 48 kHz stereo Float32 on stdout, never microphone input.
import Foundation
import ScreenCaptureKit
import AVFoundation
import CoreMedia

final class AudioOutput: NSObject, SCStreamOutput {
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, sampleBuffer.isValid, let description = sampleBuffer.formatDescription else { return }
        let format = AVAudioFormat(cmAudioFormatDescription: description)
        var size = 0
        guard CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sampleBuffer, bufferListSizeNeededOut: &size, bufferListOut: nil, bufferListSize: 0, blockBufferAllocator: nil, blockBufferMemoryAllocator: nil, flags: 0, blockBufferOut: nil) == noErr else { return }
        let memory = UnsafeMutableRawPointer.allocate(byteCount: size, alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { memory.deallocate() }
        let list = memory.bindMemory(to: AudioBufferList.self, capacity: 1)
        var retained: CMBlockBuffer?
        guard CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sampleBuffer, bufferListSizeNeededOut: nil, bufferListOut: list, bufferListSize: size, blockBufferAllocator: nil, blockBufferMemoryAllocator: nil, flags: 0, blockBufferOut: &retained) == noErr,
              let pcm = AVAudioPCMBuffer(pcmFormat: format, bufferListNoCopy: list),
              let channels = pcm.floatChannelData else { return }
        let count = CMSampleBufferGetNumSamples(sampleBuffer)
        guard count > 0, count <= 48000, format.channelCount >= 1, format.sampleRate == 48000 else { return }
        var samples = [Float](); samples.reserveCapacity(count * 2)
        for i in 0..<count {
            if format.isInterleaved {samples.append(channels[0][i * Int(format.channelCount)]);samples.append(channels[0][i * Int(format.channelCount) + (format.channelCount > 1 ? 1 : 0)])}
            else {samples.append(channels[0][i]);samples.append(channels[format.channelCount > 1 ? 1 : 0][i])}
        }
        samples.withUnsafeBytes { bytes in FileHandle.standardOutput.write(Data(bytes)) }
    }
}
@main struct Capture {
 static func main() async {
  do {
   let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: false)
   guard let display=content.displays.first else {throw NSError(domain:"RemvoraAudio",code:1)}
   let filter=SCContentFilter(display:display,excludingWindows:[])
   let config=SCStreamConfiguration();config.width=2;config.height=2;config.minimumFrameInterval=CMTime(value:1,timescale:1)
   config.capturesAudio=true;config.sampleRate=48000;config.channelCount=2;config.excludesCurrentProcessAudio=false
   let output=AudioOutput();let stream=SCStream(filter:filter,configuration:config,delegate:nil)
   try stream.addStreamOutput(output,type:.audio,sampleHandlerQueue:DispatchQueue(label:"remvora.audio"))
   try await stream.startCapture()
   while !Task.isCancelled {try await Task.sleep(nanoseconds:1_000_000_000)}
   try await stream.stopCapture()
  } catch {FileHandle.standardError.write(Data("System audio capture unavailable\n".utf8));exit(1)}
 }
}
