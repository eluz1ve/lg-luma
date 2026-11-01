#!/usr/bin/env swift
// Turns a macOS Dynamic Desktop picture (.heic) into a Luma dynamic wallpaper (.lumadyn), to add
// on the TV like any background: Settings › Background › Upload, then choose the file.
//
//   swift tools/lumadyn.swift "The Desert.heic" [The Desert.lumadyn]
//
// Each frame is scaled to fill 1920 × 1080 and saved as JPEG, with where it belongs in the day:
// the sun's altitude and azimuth (`apple_desktop:solar`), a time of day (`apple_desktop:h24`), or
// light and dark (`apple_desktop:apr`). Pictures without any are spread evenly over the day.
// The pack is `LUMADYN1`, a line of JSON describing the frames, then the JPEGs back to back.
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let width = 1920
let height = 1080

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

let arguments = CommandLine.arguments.dropFirst()
guard let input = arguments.first else {
    fail("usage: swift tools/lumadyn.swift <picture.heic> [output.lumadyn]")
}
let inputURL = URL(fileURLWithPath: input)
let outputURL = arguments.count > 1
    ? URL(fileURLWithPath: arguments[arguments.startIndex + 1])
    : inputURL.deletingPathExtension().appendingPathExtension("lumadyn")

guard let source = CGImageSourceCreateWithURL(inputURL as CFURL, nil) else {
    fail("Cannot open \(input)")
}
let count = CGImageSourceGetCount(source)
guard count > 0 else { fail("\(input) has no images") }

/// The Dynamic Desktop tags (`solar`, `h24`, `apr`), decoded from their base64 property lists.
func desktopTags() -> [String: [String: Any]] {
    guard let metadata = CGImageSourceCopyMetadataAtIndex(source, 0, nil),
        let tags = CGImageMetadataCopyTags(metadata) as? [CGImageMetadataTag]
    else { return [:] }
    var result: [String: [String: Any]] = [:]
    for tag in tags {
        guard let name = CGImageMetadataTagCopyName(tag) as String?,
            let value = CGImageMetadataTagCopyValue(tag) as? String,
            let data = Data(base64Encoded: value),
            let plist = try? PropertyListSerialization.propertyList(from: data, format: nil),
            let dictionary = plist as? [String: Any]
        else { continue }
        result[name] = dictionary
    }
    return result
}

func number(_ value: Any?) -> Double? {
    (value as? NSNumber)?.doubleValue
}

/// (image index, where it belongs in the day) for each frame, in the picture's order.
var frames: [(Int, [String: Double])] = []
let tags = desktopTags()
if let solar = tags["solar"]?["si"] as? [[String: Any]] {
    for entry in solar {
        guard let index = number(entry["i"]).map(Int.init), index < count,
            let altitude = number(entry["a"]), let azimuth = number(entry["z"]),
            !frames.contains(where: { $0.0 == index })
        else { continue }
        frames.append((index, ["altitude": altitude, "azimuth": (azimuth + 360).truncatingRemainder(dividingBy: 360)]))
    }
}
if frames.isEmpty, let timed = tags["h24"]?["ti"] as? [[String: Any]] {
    for entry in timed {
        guard let index = number(entry["i"]).map(Int.init), index < count,
            let time = number(entry["t"]), !frames.contains(where: { $0.0 == index })
        else { continue }
        frames.append((index, ["time": min(max(time, 0), 1)]))
    }
}
if frames.isEmpty, let appearance = tags["apr"] {
    // Light from seven in the morning, dark from seven in the evening.
    if let light = number(appearance["l"]).map(Int.init), light < count {
        frames.append((light, ["time": 7.0 / 24]))
    }
    if let dark = number(appearance["d"]).map(Int.init), dark < count {
        frames.append((dark, ["time": 19.0 / 24]))
    }
}
if frames.isEmpty {
    frames = (0..<count).map { ($0, ["time": Double($0) / Double(count)]) }
}
if frames.count > 32 { fail("\(input) has more than 32 frames") }

/// A frame filling 1920 × 1080, cropped around its centre, as JPEG.
func jpeg(_ index: Int) -> Data {
    guard let image = CGImageSourceCreateImageAtIndex(source, index, nil),
        let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
    else { fail("Cannot read frame \(index)") }
    let scale = max(Double(width) / Double(image.width), Double(height) / Double(image.height))
    let size = CGSize(width: Double(image.width) * scale, height: Double(image.height) * scale)
    context.interpolationQuality = .high
    context.draw(
        image,
        in: CGRect(
            x: (Double(width) - size.width) / 2, y: (Double(height) - size.height) / 2,
            width: size.width, height: size.height))
    let data = NSMutableData()
    guard let scaled = context.makeImage(),
        let destination = CGImageDestinationCreateWithData(
            data, UTType.jpeg.identifier as CFString, 1, nil)
    else { fail("Cannot encode frame \(index)") }
    CGImageDestinationAddImage(
        destination, scaled, [kCGImageDestinationLossyCompressionQuality: 0.88] as CFDictionary)
    guard CGImageDestinationFinalize(destination) else { fail("Cannot encode frame \(index)") }
    return data as Data
}

var images: [Data] = []
var header: [[String: Any]] = []
for (index, position) in frames {
    let image = jpeg(index)
    images.append(image)
    var entry: [String: Any] = position
    entry["length"] = image.count
    header.append(entry)
}
var pack = Data("LUMADYN1\n".utf8)
pack.append(try! JSONSerialization.data(withJSONObject: ["frames": header], options: [.sortedKeys]))
pack.append(Data("\n".utf8))
for image in images { pack.append(image) }
if pack.count > 32 * 1024 * 1024 { fail("The pack is over 32 MiB") }
do {
    try pack.write(to: outputURL)
} catch {
    fail("Cannot write \(outputURL.path): \(error.localizedDescription)")
}
let kind = frames.first?.1["altitude"] != nil ? "by the sun" : "by the time of day"
print("\(outputURL.path): \(frames.count) frames \(kind), \(pack.count / 1024) KiB")
