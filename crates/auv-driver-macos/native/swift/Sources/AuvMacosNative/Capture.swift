import CoreGraphics
import CoreImage
import CoreMedia
import CoreVideo
import Foundation
import ScreenCaptureKit

private func emptyWindowCaptureResponse(
  message: String,
  recovery: String
) -> NativeWindowCaptureResponse {
  NativeWindowCaptureResponse(
    image_width: 0,
    image_height: 0,
    window_x: 0,
    window_y: 0,
    window_width: 0,
    window_height: 0,
    rgba_bytes: RustVec<UInt8>(),
    error_message: message.intoRustString(),
    recovery_hint: recovery.intoRustString()
  )
}

func capture_window_image(request: NativeWindowCaptureRequest) -> NativeWindowCaptureResponse {
  let image: CGImage
  let capturedFrame: CGRect
  switch nativeCaptureWindowForAuv(windowID: UInt32(max(request.window_id, 0)), logical: request.logical) {
  case .success(let captured):
    image = captured.image
    capturedFrame = captured.frame
  case .failure(let error):
    return emptyWindowCaptureResponse(
      message: "ScreenCaptureKit window capture failed: \(error)",
      recovery: "check the reported capture stage and target window; permission denial requires user authorization"
    )
  }
  guard let rgba = nativeRgbaBytes(from: image) else {
    return emptyWindowCaptureResponse(
      message: "failed to extract RGBA bytes from captured window",
      recovery: "retry capture or use a fallback capture method"
    )
  }
  return NativeWindowCaptureResponse(
    image_width: Int64(image.width),
    image_height: Int64(image.height),
    window_x: Double(capturedFrame.minX),
    window_y: Double(capturedFrame.minY),
    window_width: Double(capturedFrame.width),
    window_height: Double(capturedFrame.height),
    rgba_bytes: nativeByteVec(rgba),
    error_message: nil,
    recovery_hint: nil
  )
}

private func nativeCaptureWindowForAuv(
  windowID: UInt32,
  logical: Bool
) -> Result<(image: CGImage, frame: CGRect), Error> {
  guard #available(macOS 14.0, *) else {
    return .failure(NSError(
      domain: "AuvMacosNative.Capture",
      code: 3,
      userInfo: [NSLocalizedDescriptionKey: "ScreenCaptureKit screenshot capture requires macOS 14.0 or newer"]
    ))
  }

  let operation = WindowCaptureOperation()
  SCShareableContent.getWithCompletionHandler { content, error in
    // NOTICE: lookup callbacks can arrive after the synchronous FFI deadline.
    // Previously they still created a new screenshot request after fallback.
    guard operation.beginScreenshot() else { return }
    if let error {
      operation.finish(.failure(error))
      return
    }
    guard let window = content?.windows.first(where: { $0.windowID == windowID }) else {
      operation.finish(.failure(NSError(
        domain: "AuvMacosNative.Capture",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "window \(windowID) not found"]
      )))
      return
    }

    let filter = SCContentFilter(desktopIndependentWindow: window)
    let config = SCStreamConfiguration()
    // NOTICE(window-capture-pixel-scale): SCStreamConfiguration.width/height
    // are output pixels, while SCWindow.frame is in points. Passing points
    // captured Retina windows at 1x while display captures were 2x (measured
    // 2026-10-07). `pointPixelScale` (macOS 14+) is the backing scale of the
    // filtered content; see
    // https://developer.apple.com/documentation/screencapturekit/scshareablecontentinfo.
    let scale = logical ? 1.0 : CGFloat(SCShareableContent.info(for: filter).pointPixelScale)
    config.width = max(1, Int((window.frame.width * scale).rounded()))
    config.height = max(1, Int((window.frame.height * scale).rounded()))
    config.pixelFormat = kCVPixelFormatType_32BGRA
    config.colorSpaceName = CGColorSpace.sRGB
    config.showsCursor = false

    SCScreenshotManager.captureSampleBuffer(
      contentFilter: filter,
      configuration: config
    ) { sampleBuffer, captureError in
      if let captureError {
        operation.finish(.failure(captureError))
        return
      }
      guard
        let sampleBuffer,
        let image = nativeImageFromSampleBuffer(sampleBuffer)
      else {
        operation.finish(.failure(NSError(
          domain: "AuvMacosNative.Capture",
          code: 2,
          userInfo: [NSLocalizedDescriptionKey: "window capture returned no image sample"]
        )))
        return
      }
      operation.finish(.success((image, window.frame)))
    }
  }
  return operation.wait()
}

func nativeImageFromSampleBuffer(_ sampleBuffer: CMSampleBuffer) -> CGImage? {
  guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
    return nil
  }
  let ciImage = CIImage(cvPixelBuffer: pixelBuffer)
  return CIContext(options: nil).createCGImage(ciImage, from: ciImage.extent)
}

func nativeRgbaBytes(from image: CGImage) -> [UInt8]? {
  let width = image.width
  let height = image.height
  var bytes = [UInt8](repeating: 0, count: width * height * 4)
  guard
    let context = CGContext(
      data: &bytes,
      width: width,
      height: height,
      bitsPerComponent: 8,
      bytesPerRow: width * 4,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )
  else {
    return nil
  }
  context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
  return bytes
}

func nativeImageFromRgbaBytes(width: Int, height: Int, bytes: [UInt8]) -> CGImage? {
  guard width > 0, height > 0, bytes.count == width * height * 4 else {
    return nil
  }
  let data = Data(bytes)
  guard let provider = CGDataProvider(data: data as CFData) else {
    return nil
  }
  return CGImage(
    width: width,
    height: height,
    bitsPerComponent: 8,
    bitsPerPixel: 32,
    bytesPerRow: width * 4,
    space: CGColorSpaceCreateDeviceRGB(),
    bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
    provider: provider,
    decode: nil,
    shouldInterpolate: false,
    intent: .defaultIntent
  )
}
