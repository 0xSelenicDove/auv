import AppKit
import Foundation
let root = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let width = 700.0, height = 480.0, scale = 2.0
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(width*scale), pixelsHigh: Int(height*scale), bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
let context = NSGraphicsContext(bitmapImageRep: bitmap)!
NSGraphicsContext.saveGraphicsState(); NSGraphicsContext.current = context
context.cgContext.scaleBy(x: scale, y: scale)
NSColor.white.setFill(); NSRect(x: 0, y: 0, width: width, height: height).fill()
var expected: [[String: Any]] = []
for (i, size) in [8.0,10.0,12.0,14.0].enumerated() {
  for language in 0..<2 {
    let top = Double(i*2+language)*48+30
    let text = language == 0 ? "Small ledger EN\(Int(size))001 Ready for review" : "中文交接 ZH\(Int(size))001 已完成"
    let attributes: [NSAttributedString.Key: Any] = [.font:NSFont.systemFont(ofSize:size),.foregroundColor:NSColor.black]
    let measure = (text as NSString).size(withAttributes: attributes)
    (text as NSString).draw(at:NSPoint(x:24,y:height-top-measure.height),withAttributes:attributes)
    expected.append(["text":text,"top":top,"height":measure.height,"left":24.0,"width":measure.width,"font_points":size])
  }
}
NSGraphicsContext.restoreGraphicsState()
try bitmap.representation(using:.png,properties:[:])!.write(to:root.appendingPathComponent("small-mixed.png"))
let metadata: [String:Any] = ["path":"small-mixed.png","logical_width":width,"logical_height":height,"scale":scale,"expected":expected]
try JSONSerialization.data(withJSONObject:metadata,options:[.prettyPrinted,.sortedKeys]).write(to:root.appendingPathComponent("small-mixed.json"))
