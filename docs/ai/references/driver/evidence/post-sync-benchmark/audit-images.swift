import AppKit
import Vision

// Read saved evidence only. This audit neither captures nor controls the desktop.
for path in CommandLine.arguments.dropFirst() {
  guard let image = NSImage(contentsOfFile: path),
        let pixels = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    fatalError("Cannot decode evidence: \(path)")
  }
  let request = VNRecognizeTextRequest()
  request.recognitionLevel = .accurate
  request.usesLanguageCorrection = false
  try VNImageRequestHandler(cgImage: pixels).perform([request])
  let rows = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
  let result: [String: Any] = ["path": path, "width": pixels.width, "height": pixels.height, "rows": rows]
  let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
  print(String(data: data, encoding: .utf8)!)
}
