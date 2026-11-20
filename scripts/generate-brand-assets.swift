// Deterministic crop and resize of the supplied logo; no artwork is redrawn.
// Run from the repository root: swift scripts/generate-brand-assets.swift
import AppKit
import Foundation

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let destination = root.appendingPathComponent("assets/brand", isDirectory: true)
let source = destination.appendingPathComponent("source.png")
guard let bitmap = NSBitmapImageRep(data: try Data(contentsOf: source)),
      let original = bitmap.cgImage,
      original.width == 1983, original.height == 793 else {
    fatalError("Expected the supplied 1983 × 793 ShrimpAPI.png")
}

func writePNG(_ image: CGImage, _ name: String) throws {
    let representation = NSBitmapImageRep(cgImage: image)
    guard let data = representation.representation(using: .png, properties: [:]) else {
        fatalError("Cannot encode \(name)")
    }
    try data.write(to: destination.appendingPathComponent(name), options: .atomic)
}

// Remove outer transparent margins while retaining the original full wordmark.
let logo = original.cropping(to: CGRect(x: 160, y: 145, width: 1660, height: 500))!
try writePNG(logo, "logo.png")

// The wordmark starts to the right of x=630; keep only the shrimp and arrows.
let mark = original.cropping(to: CGRect(x: 160, y: 145, width: 470, height: 500))!
try writePNG(mark, "mark.png")

let side = 1024
let context = CGContext(
    data: nil, width: side, height: side, bitsPerComponent: 8,
    bytesPerRow: side * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!
context.setAllowsAntialiasing(true)
context.setShouldAntialias(true)
context.setFillColor(CGColor(gray: 1, alpha: 1))
context.addPath(CGPath(
    roundedRect: CGRect(x: 64, y: 64, width: 896, height: 896),
    cornerWidth: 200, cornerHeight: 200, transform: nil
))
context.fillPath()
context.interpolationQuality = .high
let height: CGFloat = 728
let width = height * CGFloat(mark.width) / CGFloat(mark.height)
context.draw(mark, in: CGRect(x: (1024 - width) / 2, y: (1024 - height) / 2, width: width, height: height))
try writePNG(context.makeImage()!, "icon.png")
print("Generated logo.png (1660 × 500), mark.png (470 × 500), icon.png (1024 × 1024)")
