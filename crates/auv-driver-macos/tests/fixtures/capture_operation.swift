import CoreGraphics
import Foundation

func failure(_ result: Result<(image: CGImage, frame: CGRect), Error>) -> NSError {
  switch result {
  case .failure(let error): return error as NSError
  case .success: fatalError("expected failure")
  }
}

@main
struct CaptureDeadlineTests {
  static func main() throws {
    // ROOT CAUSE: lookup callbacks arriving after the synchronous FFI timeout
    // still started screenshots, overlapping fallback and corrective recovery.
    let expired = WindowCaptureOperation(timeout: .nanoseconds(0))
    let original = failure(expired.wait())
    assert(original.localizedDescription.contains("shareable-content lookup"))
    assert(!expired.beginScreenshot())
    expired.finish(.failure(NSError(domain: "late callback", code: 1)))
    assert(failure(expired.wait()).localizedDescription == original.localizedDescription)

    // A callback may win the lock after the deadline but before wait returns.
    let late = WindowCaptureOperation(timeout: .nanoseconds(0))
    late.finish(.failure(NSError(domain: "late callback", code: 1)))
    assert(failure(late.wait()).localizedDescription.contains("shareable-content lookup"))

    let screenshot = WindowCaptureOperation(timeout: .milliseconds(50))
    assert(screenshot.beginScreenshot())
    assert(failure(screenshot.wait()).localizedDescription.contains("screenshot delivery"))
    assert(!screenshot.beginScreenshot())

    let ready = WindowCaptureOperation()
    assert(ready.beginScreenshot())
    let context = CGContext(data: nil, width: 2, height: 2, bitsPerComponent: 8, bytesPerRow: 8,
                            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let image = context.makeImage()!
    let frame = CGRect(x: 10, y: 20, width: 1, height: 1)
    ready.finish(.success((image, frame)))
    ready.finish(.failure(NSError(domain: "duplicate callback", code: 1)))
    let captured = try ready.wait().get()
    assert(captured.image === image && captured.frame == frame)
    assert(!ready.beginScreenshot())
    print("capture deadline: late lookup, late completion, stage timeout and exact result passed")
  }
}
