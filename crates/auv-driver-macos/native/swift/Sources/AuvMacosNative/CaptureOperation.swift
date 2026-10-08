import CoreGraphics
import Foundation

// Own the callback result through one monotonic deadline. A timed-out lookup
// must not start screenshot work while the caller is already using fallback.
// NOTICE: ScreenCaptureKit supplies no cancellation handle for screenshots;
// late delivery is discarded. See `SCScreenshotManager.captureSampleBuffer`:
// https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/capturesamplebuffer(contentfilter:configuration:completionhandler:)
final class WindowCaptureOperation {
  private let lock = NSLock()
  private let semaphore = DispatchSemaphore(value: 0)
  private let deadline: DispatchTime
  private var stage = "shareable-content lookup"
  private var result: Result<(image: CGImage, frame: CGRect), Error>?

  init(timeout: DispatchTimeInterval = .seconds(10)) {
    deadline = .now() + timeout
  }

  func beginScreenshot() -> Bool {
    lock.lock()
    defer { lock.unlock() }
    guard result == nil, DispatchTime.now() < deadline else { return false }
    stage = "screenshot delivery"
    return true
  }

  func finish(_ value: Result<(image: CGImage, frame: CGRect), Error>) {
    lock.lock()
    defer { lock.unlock() }
    guard result == nil, DispatchTime.now() < deadline else { return }
    result = value
    semaphore.signal()
  }

  func wait() -> Result<(image: CGImage, frame: CGRect), Error> {
    _ = semaphore.wait(timeout: deadline)
    lock.lock()
    defer { lock.unlock() }
    if let result { return result }
    let failure = NSError(
      domain: "AuvMacosNative.Capture",
      code: 4,
      userInfo: [NSLocalizedDescriptionKey: "\(stage) timed out before the capture deadline"]
    )
    let value: Result<(image: CGImage, frame: CGRect), Error> = .failure(failure)
    result = value
    return value
  }
}

