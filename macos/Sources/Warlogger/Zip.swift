import Compression
import Foundation

private let table: [UInt32] = (0..<256).map { i in
    (0..<8).reduce(UInt32(i)) { c, _ in c & 1 != 0 ? 0xEDB8_8320 ^ (c >> 1) : c >> 1 }
}

private func crc32(_ data: Data) -> UInt32 {
    ~data.reduce(~UInt32(0)) { c, b in table[Int((c ^ UInt32(b)) & 0xFF)] ^ (c >> 8) }
}

private func deflate(_ data: Data) -> Data {
    let capacity = data.count + data.count / 10 + 1024
    var out = Data(count: capacity)
    let size = out.withUnsafeMutableBytes { dst in
        data.withUnsafeBytes { src in
            compression_encode_buffer(dst.bindMemory(to: UInt8.self).baseAddress!, capacity,
                                      src.bindMemory(to: UInt8.self).baseAddress!, data.count, nil, COMPRESSION_ZLIB)
        }
    }
    return out.prefix(size)
}

private extension Data {
    mutating func le<T: FixedWidthInteger>(_ value: T) {
        Swift.withUnsafeBytes(of: value.littleEndian) { append(contentsOf: $0) }
    }
}

func zip(_ text: String) -> Data {
    let data = Data(text.utf8)
    let packed = deflate(data)
    let name = Data("log.txt".utf8)
    let (crc, size, len) = (crc32(data), UInt32(data.count), UInt32(packed.count))

    func header(_ out: inout Data, _ signature: UInt32, central: Bool) {
        out.le(signature)
        if central { out.le(UInt16(20)) }
        for field: UInt16 in [20, 0, 8, 0, 0x21] { out.le(field) }
        for field in [crc, len, size] { out.le(field) }
        out.le(UInt16(name.count))
        out.le(UInt16(0))
        if central {
            out.append(Data(count: 6))
            out.le(UInt32(0))
            out.le(UInt32(0))
        }
        out.append(name)
    }

    var out = Data(capacity: packed.count + 128)
    header(&out, 0x0403_4B50, central: false)
    out.append(packed)

    let directory = UInt32(out.count)
    header(&out, 0x0201_4B50, central: true)
    let length = UInt32(out.count) - directory

    out.le(UInt32(0x0605_4B50))
    out.append(Data(count: 4))
    out.le(UInt16(1))
    out.le(UInt16(1))
    out.le(length)
    out.le(directory)
    out.le(UInt16(0))
    return out
}
