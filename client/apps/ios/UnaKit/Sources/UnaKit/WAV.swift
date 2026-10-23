import Foundation

/// 16 kHz mono 16-bit PCM WAV, the format the desktop client sends.
public enum WAV {
    public static let sampleRate = 16_000
    public static let headerSize = 44

    public static func encode(_ samples: [Int16], sampleRate: Int = WAV.sampleRate) -> Data {
        let dataSize = samples.count * 2
        var d = Data(capacity: headerSize + dataSize)
        func u32(_ v: Int) { var x = UInt32(v).littleEndian; withUnsafeBytes(of: &x) { d.append(contentsOf: $0) } }
        func u16(_ v: Int) { var x = UInt16(v).littleEndian; withUnsafeBytes(of: &x) { d.append(contentsOf: $0) } }
        d.append(contentsOf: Array("RIFF".utf8)); u32(36 + dataSize)
        d.append(contentsOf: Array("WAVE".utf8))
        d.append(contentsOf: Array("fmt ".utf8)); u32(16); u16(1); u16(1)
        u32(sampleRate); u32(sampleRate * 2); u16(2); u16(16)
        d.append(contentsOf: Array("data".utf8)); u32(dataSize)
        samples.withUnsafeBufferPointer { p in
            for s in p { var x = s.littleEndian; withUnsafeBytes(of: &x) { d.append(contentsOf: $0) } }
        }
        return d
    }

    /// Float samples in [-1, 1] at `inputRate` -> Int16 at 16 kHz (linear interpolation, with a
    /// box low-pass when downsampling so 48 kHz input doesn't alias).
    public static func resampleTo16k(_ input: [Float], inputRate: Double) -> [Int16] {
        guard !input.isEmpty, inputRate > 0 else { return [] }
        let ratio = inputRate / Double(sampleRate)
        let outCount = Int((Double(input.count) / ratio).rounded(.down))
        var out = [Int16](); out.reserveCapacity(outCount)
        let window = max(1, Int(ratio.rounded()))
        for i in 0..<outCount {
            let pos = Double(i) * ratio
            var acc: Float
            if ratio > 1.5 {
                let start = Int(pos), end = min(input.count, start + window)
                var sum: Float = 0
                for j in start..<end { sum += input[j] }
                acc = sum / Float(max(1, end - start))
            } else {
                let i0 = Int(pos), i1 = min(input.count - 1, i0 + 1)
                let f = Float(pos - Double(i0))
                acc = input[i0] * (1 - f) + input[i1] * f
            }
            out.append(Int16(max(-1, min(1, acc)) * 32767))
        }
        return out
    }

    public static func durationMs(sampleCount: Int) -> Int { sampleCount * 1000 / sampleRate }

    /// Reads a PCM16 mono WAV (as written by `encode`, or afconvert) back into samples and rate.
    public static func decode(_ data: Data) -> (samples: [Int16], sampleRate: Int)? {
        let b = [UInt8](data)
        guard b.count >= 12, String(bytes: b[0..<4], encoding: .ascii) == "RIFF",
              String(bytes: b[8..<12], encoding: .ascii) == "WAVE" else { return nil }
        func u32(_ o: Int) -> Int { Int(b[o]) | Int(b[o+1]) << 8 | Int(b[o+2]) << 16 | Int(b[o+3]) << 24 }
        var pos = 12, rate = 0, bits = 0, channels = 0
        while pos + 8 <= b.count {
            let id = String(bytes: b[pos..<pos+4], encoding: .ascii) ?? ""
            let size = u32(pos + 4)
            if id == "fmt " {
                channels = Int(b[pos+10]) | Int(b[pos+11]) << 8
                rate = u32(pos + 12)
                bits = Int(b[pos+22]) | Int(b[pos+23]) << 8
            } else if id == "data" {
                guard bits == 16, channels >= 1 else { return nil }
                let end = min(b.count, pos + 8 + size)
                var s = [Int16](); var o = pos + 8
                while o + 2 * channels <= end {
                    s.append(Int16(bitPattern: UInt16(b[o]) | UInt16(b[o+1]) << 8)); o += 2 * channels
                }
                return (s, rate)
            }
            pos += 8 + size + (size & 1)
        }
        return nil
    }
}
