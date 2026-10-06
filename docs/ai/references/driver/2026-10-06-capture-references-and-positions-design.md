# Capture references and positions

Status: proposed (2026-10-06). Names marked *provisional* are open for review.

This note proposes two related simplifications of the driver API:

- **Capture references.** Captured pixels stay in the Runner. Calls exchange
  references and metadata, and pixels come back only when a caller explicitly
  asks for them.
- **Positions.** Use one coordinate model across input, OCR and regions. It is
  built on the existing `Position` contract.

Backward compatibility is not a goal (AUV 0.0.29). Each slice changes the
Protobuf contract, the Runner, the Rust client, the JS SDK, and their callers
together.

## Evidence

The evidence comes from building `devtools/repl-playground` (#249, #250)
against a local daemon, with read-only measurements on macOS.

- **Captures are large.**
  - A Retina window capture is 3292×1932 RGBA, about 25 MB; a display capture
    is about 80 MB.
  - `capture()` takes 0.7–0.9 s over HTTP, plus about 60–70 ms of protobuf
    decode on the browser main thread.
  - The playground had to add a raw-pixel memory budget, decode at logical
    resolution, and keep a cached canvas layer to stay responsive.
- **`RecognizeText` takes the capture back as input.**
  `RecognizeTextRequest.capture` is a full `CapturedFrame`. OCR on a capture
  therefore downloads 25 MB and uploads the same 25 MB.
- **`FindWindowText`/`FindDisplayText` always return the capture.** The proto
  comment says this is so that "clients may persist it as an artifact". That
  pushes evidence storage onto every client.
- **`ScrollUntil` observations carry a capture and OCR by default** for every
  step.
- **Coordinates come in several flavors for one concept.**
  - Input is split by space: `ClickScreenPoint` versus `ClickWindowPoint`,
    `ScrollWindowPoint*` (window-local only), and `MoveMouse`/`DragMouse`
    (screen).
  - OCR bounds are typed `ScreenRect`, but they are offsets from `origin`.
  - Regions are 0–1 fractions (`NormalizedRect`).
  - The playground converts screen areas to fractions and to window-local
    points.
  - `auv-netease-music` converts bounds to ratios (`bounds_to_ratio`).
  - The same normalized rectangle is defined three times: `RatioRect`
    (`auv-driver-common`), `NormalizedRegion` (`auv-core` client) and proto
    `NormalizedRect`.

The rule this design follows is now in `AGENTS.md` ("Image Payloads", #251).

## Part A — Capture references

### Terms

- **Capture reference** (`CaptureRef { capture_id }`, *provisional*): a
  Runner resource. It names one capture held in that Runner's capture store
  (see [resource reference scope](../../../TERMS_AND_CONCEPTS.md#resource-reference-scope)).
  Like `FrameBufferRef`, it is valid only on a Run-affine route to the same
  Runner. The local Runner (`auv.core.local`) is persistent, so local
  references work across Runs.
- **Capture store** (*provisional*): an in-memory, least-recently-used cache in
  the Runner, bounded by bytes.
  - It holds every capture the Runner produces: window, display and region
    captures, find-text evidence, and scroll-until steps.
  - An evicted or unknown reference fails with `NOT_FOUND` ("capture
    evicted").
  - Default budget: 512 MiB. That is about 20 Retina window captures or 6
    display captures.
  - There is no release RPC in this slice. Eviction and Runner shutdown are the
    only ways a capture leaves the store.
- **Capture image fetch**: an explicit request for pixels, possibly bounded and
  encoded.

### Protobuf changes (`capture.proto`, `text_recognition.proto`, `input.proto`)

```proto
message CaptureRef { string capture_id = 1; }

message CapturedFrame {
  CaptureRef ref = 7;                       // always set by the Runner
  auv.api.image.v1.RgbaFrame image = 1;     // only when explicitly requested
  ScreenRect bounds = 2;
  double scale_factor = 3;
  string backend = 4;
  optional string fallback_reason = 5;
  Position origin = 6;
  auv.api.image.v1.PixelSize pixel_size = 8; // known without pixels
}

service CaptureService {
  // existing Capture* RPCs: response capture has `ref`, no `image` by default
  rpc GetCaptureImage(GetCaptureImageRequest) returns (GetCaptureImageResponse);
}

message GetCaptureImageRequest {
  CaptureRef capture = 1;
  optional auv.api.image.v1.NormalizedRect region = 2; // crop first
  auv.api.image.v1.PixelSize max_size = 3;             // fit inside; absent = native
  ImageEncoding encoding = 4;                          // RGBA (default), PNG, JPEG
}
message GetCaptureImageResponse {
  oneof image {
    auv.api.image.v1.RgbaFrame rgba = 1;
    EncodedImage encoded = 2;   // bytes + format + pixel size
  }
}

message RecognizeTextRequest {
  oneof source {
    CaptureRef capture_ref = 6;   // AUV-produced capture: no pixels sent back
    CapturedFrame capture = 2;    // caller-owned image only
  }
  ...
}
```

The capture RPCs, the find-text responses and `ScrollUntilObservation.capture`
return `CapturedFrame` with `ref` and metadata, without `image`.
`ScrollUntilObserve.omit_capture` is replaced by nothing: references cost
nothing to return. A caller that wants pixels makes an explicit
`GetCaptureImage` call.

Two deferrals keep this slice focused:

- `TODO(capture-store-release)`: there is no explicit release RPC. Add one
  when a long-running client needs to free memory before eviction.
- `TODO(recent-frames-capture-refs)`: `GetRecentFrames` still returns frames
  with pixels. Frames should enter the capture store and travel as
  references. They will share that path with the planned video stream.

### Consumers

| Consumer | Today | After |
|---|---|---|
| Rust `auv-core` client | `WindowCapture.capture: auv_driver::Capture` (pixels) | A capture value with `reference`, bounds, origin, scale and pixel size; `runner.captures().image(&reference, options)` returns `auv_driver::Capture` when pixels are needed |
| Rust `recognize_text` | Takes `auv_driver::Capture` | Takes a capture reference, or a caller-owned `Capture` |
| `auv-cli-invoke` artifacts (`display.capture`, `screen.captureRegion`, find-text) | Writes a PNG from response pixels | Explicitly fetches PNG-encoded bytes via `GetCaptureImage` |
| `auv-game-balatro` OCR | Captures, then `recognize_text(capture)` (round trip) | `recognize_text(reference, region)` |
| JS SDK | `capture()` returns pixels; `recognizeText(frame)` | `capture()` returns metadata plus `ref`; `runner.captures.image(ref, { maxSize, encoding })`; `recognizeText(ref)` |
| repl-playground | Decodes 25 MB RGBA per capture; keeps raw pixels within a budget for OCR | Fetches a logical-resolution PNG/JPEG thumbnail; `createImageBitmap(blob)` decodes off the main thread; OCR by reference; the raw-frame budget is deleted |

Run recording keeps its current shape in this slice. `auv-cli-invoke`
persists PNG artifacts, now by explicit fetch. Persisting evidence on the AUV
side from references is a follow-up (`TODO(runner-side-capture-artifacts)`).

## Part B — Positions

The domain already has the right model:

- `Position` is a point plus its `CoordinateSpace`: screen, display or window.
- `Positional` supplies a position without IO.
- `Capture.origin` and `TextRecognition.origin` tie images and OCR results to
  their space, and `relative_to()` rebases them (see
  [Position and Positional](../../../TERMS_AND_CONCEPTS.md#position-and-positional)).
- #174 already merged CLI clicks into
  `input.clickPoint --relative-to screen|window|display`.

The wire and the SDK still split everything by space. Proposal:

1. **Input takes `Position`.** `ClickPoint { Position position; ClickOptions }`
   replaces `ClickScreenPoint` and `ClickWindowPoint`.
   - A window position uses window-targeted delivery, and a screen or display
     position uses global delivery.
   - Window-scoped calls (`WindowClient.click`/`scroll*`) accept a position in
     window or screen space. The Runner converts screen positions using the
     window's current frame, so callers stop converting by hand.
   - `MoveMouse`/`DragMouse` keep screen points.
2. **Everything AUV returns is in screen space.** For AUV-produced captures,
   `RecognizedText.bounds` and `TextMatch.bounds` are screen rectangles, as
   their type already claims. The Runner applies `origin` before responding.
   Offsets relative to `origin` remain only for caller-owned images, which have
   no screen placement.
3. **Regions accept a screen rectangle.** Every `region` field becomes a
   `oneof { NormalizedRect normalized; ScreenRect screen; }`. A screen
   rectangle is mapped into the image by the Runner and clipped to it. A
   rectangle that misses the image entirely is `INVALID_ARGUMENT`. Clients pass
   their areas directly.
4. **One normalized rectangle.**
   - Delete `auv-core`'s `NormalizedRegion` in favor of `auv-driver-common`'s
     type.
   - Rename `RatioRect` to `NormalizedRect` (*provisional*), so that Rust,
     Protobuf and TypeScript use one name.
   - `auv-view`'s `ViewBounds` stays for its documented dependency direction.
5. **SDK geometry helpers.** The JS SDK exports small pure helpers that both
   apps and the playground use instead of local copies:
   - `center(rect)`;
   - `Position.screen(x, y)` and `Position.window(windowOrRef, x, y)`;
   - `contains`, and `intersect`/`clip`.

   The playground's `area()` builds on these.

## Slices and order

1. **Capture references (Part A).** Proto, Runner store, `GetCaptureImage`,
   `RecognizeText(capture_ref)`, Rust client, invoke artifacts, Balatro, JS SDK
   and playground. The playground's raw-frame budget and its main-thread RGBA
   decode are deleted.
2. **Screen-space results and screen regions (Part B.2–B.3).** This removes the
   playground's `NOTICE(ocr-region-space)` and its screen-to-normalized
   conversion.
3. **`Position` input (Part B.1)** and the duplicate-type cleanup (B.4), plus
   the SDK helpers (B.5).

Each slice updates `TERMS_AND_CONCEPTS.md`, the SDK README and
`SUPPORT_MATRIX.md` where they describe the changed shape.

## Documents this supersedes

- `TERMS_AND_CONCEPTS.md` → **Capture Frame** says the caller decides whether
  to persist pixels. After slice 1, a capture is a Runner resource, and pixels
  leave the Runner only on explicit request.
- `proto/auv/api/driver/v1/text_recognition.proto` comment on
  `FindWindowTextResponse.capture` ("clients may persist it as an artifact").
- `docs/archive/verticals/session-api/2026-07-31-daemon-session-api-architecture.md`
  (archived) describes `RecognizeText` consuming a typed capture. It is already
  archived and needs no change.

## Open questions

- **Names.** `CaptureRef`, "capture store" and `GetCaptureImage` are
  provisional.
- **Store budget.** Is 512 MiB right? Should it be a Runner option?
- **Encodings.** Should JPEG quality be fixed (for example 85), or a request
  field?
- **Release RPC.** Should the store ever need explicit release, or is LRU
  eviction enough until the video stream lands?
