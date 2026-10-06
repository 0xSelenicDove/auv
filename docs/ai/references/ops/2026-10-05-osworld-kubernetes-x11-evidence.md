# OSWorld on Kubernetes with the AUV X11 driver

Date: 2026-10-05. Classification: owner-approved live validation and deployment
design. Cluster: `k8s.ihome.cat`, kubeconfig
`/Users/neko/.kube/config.d/ihome.conf`.

For copy-paste operating steps, see the companion
[Kubernetes runbook](2026-10-05-osworld-kubernetes-runbook.md).

## Verdict

AUV's X11 computer-use seam works in Kubernetes through both requested
topologies:

1. a paired HTTP Device controlled by an AUV client outside the cluster; and
2. a co-located client using the daemon's Unix socket and the desktop's X11
   socket without remote pairing.

Both paths captured the 1920x1080 XFCE desktop through `xcap.x11` and delivered
click, keyboard, wheel, and sampled drag events to an independent Tk receiver.
The official KVM-backed OSWorld V1 and OSWorld-V2.1 images then each completed
task `7767eef2-56a3-4cea-8c9f-48c070c7d65b` with an upstream evaluator score of
`1.0`; V2 completed it once through each requested topology from independent
ephemeral overlays. This is one real benchmark task, not a claim that the full
suite passes. Full-suite evaluation additionally requires every task asset,
the V2 mocked websites and GitLab, reset scheduling, and a typed AUV action
adapter.

The recommended progression is:

1. Keep the current direct Xorg Pod as the fast AUV contract fixture.
2. Use the now-validated direct QEMU Pod on `liet-gpu-1` for the first task
   subset, with a cold archive PVC, node-local hot qcow2, and `/dev/kvm`.
3. Install KubeVirt plus CDI only when VM-native import, snapshots, cloning,
   or declarative VM lifecycle justify the extra controllers.

Do not require a GPU for the benchmark desktop. The validated dummy Xorg used
llvmpipe despite a HAMi allocation, and HAMi's container CUDA allocation does
not become a guest GPU. KubeVirt GPU passthrough is a separate host-device
contract and should not be mixed into the first OSWorld lane.

## Evidence boundary

The live desktop fixture was an XFCE session on dummy Xorg `:99` in Pod
`auv-osworld-x11`, namespace `auv-x11-hami-test`, node `neko-gpu-1`. AUV
`0.0.27` was built from PR #233 on a persistent `tns-iscsi` workspace PVC.
The Pod was allocated a HAMi ResourceClaim, but the display was not rendered by
the GPU.

No CUA implementation, CUA REPL, VNC input, PyAutoGUI input, `xdotool`, or
OSWorld `/execute` action endpoint was used for the evidence below. Kubernetes
commands prepared and inspected the fixture; all observed desktop capture and
input delivery went through the installed AUV binary.

### Group 1: paired remote Device

The daemon listened on Unix and HTTP endpoints inside the desktop container.
The HTTP endpoint was forwarded to the Mac, enrolled as profile
`osworld-x11-remote`, and selected by the remote Device's actual name
`auv-osworld-x11`:

```bash
auv --device auv-osworld-x11 invoke display.list --json
auv --device auv-osworld-x11 invoke input.clickPoint 750 350 --json
auv --device auv-osworld-x11 invoke input.keys control a --json
auv --device auv-osworld-x11 invoke input.typeText group1-remote-paired --json
auv --device auv-osworld-x11 invoke input.clickPoint 750 445 --json
auv --device auv-osworld-x11 invoke input.scrollPoint 750 670 0 6 --settle-ms 200 --json
auv --device auv-osworld-x11 invoke display.capture --json
```

The capture Run was `c489cdce-c0a1-554e-d3d9-04738a4a07a0`; its PNG SHA256
was `084e0153850d82215a4bc05ab3693a8a8a3cf65fa61107bf227d514ec9d8e47a`.
The independent receiver observed the typed characters, a second button click,
six additional Button-5 wheel events, and `scroll_total = -1440` after the two
groups. Tk reports a down-scroll detent as `-120`, while AUV's public request
uses positive Y for down.

The first remote invocation with a local AUV `0.0.26` failed while the daemon
was `0.0.27`. Pairing inventory was not a sufficient compatibility check;
remote Run creation required matching protocol versions. Rebuilding the client
at `0.0.27` and selecting the Device name rather than the local profile name
closed the failure.

### Group 2: co-located shared sockets

The second container mounted both shared paths:

```text
/run/auv/auv.sock
/tmp/.X11-unix/X99
```

Its required environment was:

```bash
export AUV_ENDPOINT=unix:///run/auv/auv.sock
export DISPLAY=:99
export XDG_SESSION_TYPE=x11
```

Sharing only `auv.sock` is insufficient. The daemon is the control plane, but
the first-party local Runner process is hosted by the invoking AUV client. That
client must therefore be able to open the selected display backend as well.
With both sockets shared, the sidecar delivered the same click/type/scroll
sequence and captured the desktop. The receiver recorded
`text = "group2-shared-socket"`, `button_clicks = 1`, and six Button-5 wheel
events. The capture PNG SHA256 was
`e2606af5aff136449527c74886f215d79edaf7e1a91c02807b6df448a1b66b02`.

This topology does not cross a VM boundary. A Kubernetes sidecar cannot share
the X11 socket inside a QEMU or KubeVirt guest. For an official OSWorld VM, the
equivalent non-paired topology must run both the AUV daemon and the harness
client inside the guest, sharing a guest Unix socket and guest X11 socket.

## Official OSWorld-V2.1 task result

The pinned V2.1 Ubuntu qcow2 and runtime image were then exercised on
`liet-gpu-1`. The release archive matched SHA256
`14b08aa7ba6c023ecb91d46de8df5de32af4d1d6bd75ea925519caf9677fc8b3`.
The running QEMU command included `accel=kvm`, `-enable-kvm`, and `-cpu host`.
The guest reported Ubuntu 22.04.3, Xorg display `:0`, and a 1920x1080
`Virtual-1` display to the installed AUV `0.0.27`.

Task `7767eef2-56a3-4cea-8c9f-48c070c7d65b` asks the harness to change GIMP's
theme from Dark to Light. It was run twice from a fresh ephemeral qcow2 overlay:

| Topology | Final capture Run | PNG SHA256 | Official evaluator |
| --- | --- | --- | --- |
| guest-local installed client without pairing | `01a10935-d0b2-76fc-b6f7-0c7107ab8434` | `bab1f60253408090fdb4c8caf8ddb4edd8fe0ff6c788b1cb7cafd35443317bf9` | `1.0` |
| paired Mac client to guest Device `a149adece47f` | `9c9ff484-d588-7355-cc3b-42d9fd68a94a` | `3e9d922625cb80f804ee85453444aff31e104aa2b2acf8cb2219709880dfe440` | `1.0` |

The guest-local socket topology was independently exercised with
`AUV_ENDPOINT=unix:///home/user/auv.sock` by Run
`01a10937-1ae6-7147-b15e-d1793dcf585b`. For each task run, OSWorld's setup API
only launched GIMP and installed or retrieved harness material. AUV performed
every screenshot, click, and `Control+Q` input. After GIMP persisted its state,
the exact upstream `check_config_status` function evaluated the retrieved
`gimprc`; both runs contained `(theme "Light")` and scored `1.0`.

The initial direct-Pod manifest failed with qemu-docker exit code 88 even though
`/dev/kvm` was mounted: a plain hostPath does not grant the container device
cgroup access. The validation Pod required `privileged: true`. A production
direct-Pod design should use a KVM device plugin or equivalent constrained
device allocation; KubeVirt's handlers already own this host-device boundary.

qemu-docker also forwards guest ports through tap/iptables for traffic sent to
the Pod IP; it does not bind the corresponding ports on the container loopback
interface. Kubernetes Services and kubelet probes worked, while direct
`kubectl port-forward` failed because it connects to container `127.0.0.1`.
A temporary in-cluster TCP proxy made the Service reachable to a local
port-forward. The runtime should also use a startup probe: its readiness briefly
reported success during guest reboot before the setup API was durably ready.

### V2.1 computer-use capability baseline (2026-10-05)

A new ephemeral overlay of the official V2.1 VM was booted on `liet-gpu-1` with
`-enable-kvm -cpu host`. Guest-local AUV `0.0.27` used its Unix endpoint;
`display.list` Run `01a10c92-fc30-741d-b5b0-4a0204439b3e` reported
`Virtual-1` at 1920x1080. `display.capture` Run
`01a10c93-1e4b-73c6-a726-ac97cf5270d7` used `xcap.x11` and produced a
2,226,580-byte PNG (SHA256
`4be2c20876b52e2da5c6e55ec98b8260ea23374bbd494fa5d61282c92c787282`).
The screenshot and desktop input came only from AUV. An independent `xev`
receiver observed these events:

| AUV input | Run | Independent X11 event |
| --- | --- | --- |
| move to `(450,300)` | `01a10c93-a37c-70a3-b54f-98f65f6336ee` | `MotionNotify` at the requested root point |
| middle and right click | `01a10c94-3252-76b7-ae36-d48a8e3f71ab`, `01a10c94-32e5-7448-aca0-33e99093bb8b` | Button-2 and Button-3 press/release pairs |
| triple click | `01a10c94-33cd-7684-8346-a95e29adcc86` | three Button-1 press/release cycles |
| scroll X=2, Y=3 | `01a10c94-7a97-7085-9696-fe13b52c92ec` | two Button-7 and three Button-5 cycles |
| hold Shift for 400 ms | `01a10c94-eb1a-7643-8309-f79f4e6e22cb` | `Shift_L` down/up 402 ms apart |
| type `AΩ中` | `01a10c95-560d-74f6-83ce-cb64ad26fc47` | `A`, `Greek_OMEGA`, and Unicode `U4E2D` events |
| press F13 | `01a10c95-aeb6-7754-a506-9bb48e1f9492` | `invalid_input` before any key press; official action list includes F13 |

The `xev` log and capture PNG were not exported from the disposable VM, so
their hashes cannot be independently recomputed from retained artifacts. The
Run IDs and event excerpts are the surviving receipt. This baseline does not
claim a V2.1 task score: the 108 release tasks and assets are gated by dataset
access, and this run did not provision mocked websites/GitLab or retest remote
pairing. Cross-call mouse/key ownership, cursor compositing, window/clipboard
APIs, and unattended agent success were also not exercised here. The test Pod,
Service, proxy, and local port-forward were removed; image PVCs remain.

The setup server's `/screenshot` took about 6.9 seconds in this run. A probe
with Kubernetes's default one-second timeout left the Pod NotReady even after
three consecutive direct screenshot successes. The runbook now gives both
startup and readiness probes a 15-second timeout; that revised manifest still
needs its own live readiness check.

## Official OSWorld V1 task result

The current V1 Ubuntu archive was also extracted to a separate node-local hot
PVC and booted with KVM. The unpinned `happysixd/osworld-docker` tag resolved at
run time to
`docker.io/happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`,
the same image digest pinned by V2.1. The extracted V1 qcow2 was
24,460,197,888 bytes.

The same GIMP task was then executed through the guest-local AUV. Its final
capture Run was `01a10958-6583-719b-8792-4c0416275c64`, with PNG SHA256
`2e3c93bcd643f184c9f1f25718a163b710e090be7c19fc1b955f6fffb6002cb9`.
After AUV delivered `Control+Q`, the V1 repository's own
`check_config_status` evaluated the retrieved `(theme "Light")` setting as
`1.0`.

The V1 archive was checked against the 12,273,896,463-byte LFS size and fully
read by `zipfile`, which validates the member CRC while extracting. Unlike the
V2 archive, its declared LFS SHA256 was not independently recomputed during
this run; do not describe the V1 archive digest as live-verified evidence. The
importer should hash while downloading on the next clean import.

### Small V1 batch-slot pilot (2026-10-05)

The same KVM node was reused for two further V1 task IDs. A fresh QEMU Pod
recreated the guest overlay between tasks; the source `System.qcow2` on
`osworld-v1-hot` stayed read-only. A Codex agent interpreted AUV screenshots and
chose typed AUV input actions. This pilot tested task reset and two applications;
it was hand-guided, not an unattended benchmark run or a representative sample.

| Task | Final AUV capture Run | Evaluator evidence |
| --- | --- | --- |
| GIMP minimum undo steps = 100, `7b7617bd-57cc-468e-9c91-40c4ec2bcb3d` | `01a10b9e-29ea-74c0-bbdc-a786e6143a7e` | retrieved `(undo-levels 100)`; upstream `check_config_status` returned `1.0` |
| Chrome enable Do Not Track, `030eeff7-b492-4218-b312-701ec99ee0cc` | `01a10c75-bc9f-71f1-a206-3fb80f8b220e` | retrieved Preferences `enable_do_not_track: true`; upstream `get_enable_do_not_track` and `exact_match` returned `1.0` |

The Chrome task was initialized with upstream `launch` steps. The agent's
navigation, toggle, confirmation, and captures used only the guest-installed
AUV; no OSWorld action `/execute`, PyAutoGUI input, VNC input, or CUA was used.
The upstream evaluator functions were extracted from the pinned V1 source for
this pilot, with retrieved guest files supplied to the getter. The complete
`DesktopEnv.evaluate()` lifecycle, including task postconfig, was not invoked.

The pilot also exposed two orchestration issues. The retained V1 and V2 hot
PVCs both store the extracted image as `System.qcow2`; using an archive member
name as Kubernetes `subPath` created an empty directory and QEMU reported no
boot disk. A single successful `/screenshot` readiness probe was insufficient
during the guest's internal reboot. The slot needs several stable API checks
and retryable uploads. On the next fresh boot, `packagekitd` briefly held the
apt lock while the Ubuntu-compatible AUV dependencies were being installed.

### V1 computer-use capability baseline (2026-10-05)

A separate official V1 VM booted from the retained read-only `System.qcow2` on
`liet-gpu-1`. The guest-installed AUV `0.0.27` used its Unix endpoint. An
independent Tk receiver recorded X11 events; all desktop capture and input
came from AUV. This is a delivery-contract test, not an OSWorld task score or
an agent/harness evaluation. The 1920x1080 `xcap.x11` capture Run was
`01a10c82-50c7-7425-87d0-7ba51c711317` (PNG SHA256
`a5a5626baa349d08e1642a5306d5053379c29509242853c0e7772c23ef7fc546`).
The receiver JSONL SHA256 was
`6005c2e411c51869f7fe6ebc3db2c07e306dd90e27bb2c8c83f67978b00e1f0c`.

| AUV input | Representative Run | Independent receiver observation |
| --- | --- | --- |
| move pointer | `01a10c83-61e0-7074-864e-0616bc5036a1` | motion to `(300,300)` |
| left, right, middle click | `01a10c83-62d1-732c-a091-ebb2452130a8`, `01a10c83-63a2-756f-8e6f-997b286e4dd7`, `01a10c84-5b0d-76c4-8898-7b97bacb9a22` | Button-1, Button-3, Button-2 press/release pairs |
| double click | `01a10c83-642e-7487-844a-87856f7e4ff8` | two Button-1 cycles and Tk double-click event |
| sampled drag | `01a10c83-65cc-76ff-afbf-de077e682eb4` | Button-1 down at `(250,400)`, 28 motion samples, up at `(600,400)` |
| vertical down/up | `01a10c83-683c-77bd-91fe-7606158a0d86`, `01a10c85-9104-7209-bcfd-7f35f3bc2946` | three Button-5 and two Button-4 wheel cycles |
| horizontal right/left | `01a10c84-5bde-704f-89bf-c3ae732ccab0`, `01a10c85-9225-77a5-9cb3-c33fcc0326dc` | two Shift+Button-5 and two Shift+Button-4 cycles in Tk |
| ASCII and Unicode text | `01a10c83-6947-751a-a549-85034c359e06`, `01a10c84-5cb7-76f3-b64d-d70ae85ec093` | `AuvV1_abc123`, `é`, and `中` key events |
| shortcut, single keys, timed hold | `01a10c83-6a7a-7374-94c6-bad77d8bc7c2`, `01a10c85-931b-7019-9e1c-8eed926acb5e`, `01a10c85-9402-74de-99a7-cafdedbc7338`, `01a10c85-94d0-706c-b08d-ed6042eecbf2`, `01a10c83-6c5f-70c4-a2ef-0ef8f24a7e1e` | Ctrl+A modifier sequence; Escape, F5, Left pairs; bounded Shift press/release |

The CLI rejects `input.clickPoint` without `<X> <Y>` and has no separate
`input.keyDown` command. Two overlapping `input.holdKeys` calls in separate CLI
processes did generate overlapping Ctrl and Shift events, but this does **not**
prove that one persistent Runner can hold independently addressable keys across
calls. The shared `KeyboardHoldController` currently permits one combination
per Runner process. These are explicit adapter/driver-contract gaps, not failed
OSWorld tasks. The temporary V1 VM, Service, and proxy were removed; hot and
cold PVCs were retained.

### Sampled-drag regression found by the live run

The first paired `input.drag 600 670 900 670 --duration-ms 400` failed with:

```text
X11 input requires finite integral coordinates or wheel detents
```

Both endpoints were integral, but the shared motion coordinator generated
fractional intermediate samples. XTEST has no subpixel coordinate. The X11
mouse-motion backend now rounds only sampled motion to the nearest root pixel;
direct click and wheel positions retain their strict lossless validation. The
Linux unit regression passed, then both live paths completed sampled drags:

| Path | Run | Receiver transition |
| --- | --- | --- |
| paired remote | `8c42cc59-af7a-4973-4a91-1de0c18f2342` | down `(600,670)`, up `(900,670)` |
| shared sockets | `01a108cc-0103-720e-83ac-7cbb9d732680` | down `(900,670)`, up `(600,670)` |

## Mapping the OSWorld action space to AUV

OSWorld and OSWorld-V2 currently publish the same GUI action list; V2 adds
`EXECUTE`. Their `DesktopEnv.step` normally sends PyAutoGUI or structured
actions to the guest server. An AUV harness should bypass that input dispatcher
and reuse only environment reset/setup, observation, and evaluation.

| OSWorld action | AUV route | Current status |
| --- | --- | --- |
| screenshot | `display.capture` | live-validated |
| `MOVE_TO` | `input.moveMouse` | live-validated in V1 and V2.1 |
| `CLICK`, `RIGHT_CLICK`, `DOUBLE_CLICK` | `input.clickPoint` button/count | left/right/middle/double live-validated in V1; right/middle/triple live-validated in V2.1; coordinate omission unsupported by CLI |
| `DRAG_TO` | `input.drag` or logical-mouse Runner calls | sampled drag live-validated after the rounding fix |
| `SCROLL` | `input.scrollPoint` | vertical/horizontal directions live-validated; OSWorld adapter must invert Y |
| `TYPING` | `input.typeText` | ASCII and Unicode live-validated |
| `PRESS`, `HOTKEY` | `input.keys` | modifier chord and several keys live-validated; baseline V2.1 binary rejected F13, repaired and live-validated on the direct Xorg fixture afterward |
| `MOUSE_DOWN`, `MOUSE_UP` | logical-mouse Runner lifecycle | implemented and Xvfb-validated; no one-shot invoke command because cross-call ownership needs a session |
| `KEY_DOWN`, `KEY_UP` | held-key Runner lifecycle | implemented and Xvfb-validated; the public invoke surface exposes bounded `holdKeys`, not arbitrary cross-process ownership |
| `WAIT`, `DONE`, `FAIL` | harness lifecycle | no driver operation required |
| V2 `EXECUTE` | OSWorld setup/evaluator control plane | deliberately not a GUI action; do not expose arbitrary shell execution as an AUV input command |

An adapter is still required because OSWorld's `DRAG_TO` starts at the current
cursor while `input.drag` names both endpoints, and down/up actions require a
stable logical input owner across calls. OSWorld's positive vertical scroll is
up, whereas AUV's positive Y scroll is down; the adapter must negate that
axis and apply the X11 step conversion measured below. OSWorld also permits a
click without coordinates at the current cursor
and accepts floating-point coordinates. AUV now exposes the current pointer
position and permits independent X11 key holds in one Runner, but the adapter
must still define coordinate rounding and retain one Runner/session for
`KEY_DOWN`/`KEY_UP` ownership. These are adapter requirements, not evidence
that any particular benchmark task failed. The adapter should be a typed
harness consumer of Driver APIs, not a parser that executes arbitrary
PyAutoGUI source.

Two observation differences remain explicit:

- xcap's X11 screenshot does not currently composite a cursor; OSWorld's guest
  screenshot path does.
- X11 window discovery/capture and clipboard paste are unsupported. Visual
  OSWorld agents can use display capture and foreground typing, but a task that
  specifically depends on clipboard semantics needs a separate approved slice.

### Follow-up capability repairs in this PR

The V2.1 baseline reproduced an `F13` parser failure before X11 delivery.
The X11 key parser now accepts F13–F24 and common OSWorld spellings for Page
Up/Down, PrintScreen, lock keys, Insert, and Pause/Break. A new build on the
direct Xorg fixture sent `input.keys f13` as Run
`01a10cad-9a49-7516-b214-013c575df5e0`; an independent `xev` window received
F13 keycode 93/keysym `0xffca` press and release. This is a driver-level repair,
not a new official-VM task score. The baseline official-VM binary predates this
change; a new Ubuntu 22.04-compatible artifact must be built before claiming
the same repair inside the V1/V2.1 guest.

The shared keyboard hold controller now admits separate, non-overlapping X11
native key identities on the same pinned display route, while the other
platforms keep their single-combination rule. Each hold still has its own
release ID, cancellation, and deadline; the Runner retains its 30-second
maximum. Common unit tests, an Xvfb/Tk real-event integration test, and an
ignored Linux Runner test exercised separate Ctrl/Shift down and up calls,
duplicate-alias rejection, release-failure recovery, shutdown, and a replacement
Runner after shutdown. That Runner test also passed against the direct Xorg
fixture; an independent `xev` receiver saw Ctrl down, Shift down, Shift up,
Ctrl up. It calls the Runner service methods across separate requests but does
not yet prove a full remote paired multi-RPC client session. The test-only
daemon, receiver, and temporary store were removed; the existing paired daemon
and PVCs remained untouched.

The read-only `input.pointerPosition` command and `GetMousePosition` Runner RPC
provide the cursor location needed for OSWorld's no-coordinate click and
current-cursor drag. Against the direct Xorg fixture, a fresh local daemon
returned `(150,150)` in Run `01a10cc1-4b83-725b-a21a-9caa54618da0`;
after `input.moveMouse 321 234` (Run
`01a10cc1-765c-75e7-86f1-d9e4042592d2`), it returned `(321,234)` in Run
`01a10cc1-9d04-761f-ab3a-d61c778f9cf3`. The existing paired Mac client
also returned `(321,234)` through the forwarded daemon in Run
`6672f98c-6eb1-db53-1178-37bbe4bc0344`. This validates the read path in
both local and paired topologies; it does not make a read-then-click sequence
atomic or prove an official VM task result.

### Scroll-step calibration after the shared scroll contract (2026-10-06)

Pinned OSWorld [V1](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/desktop_env/controllers/python.py#L406-L418)
and [V2.1](https://github.com/xlang-ai/OSWorld-V2/blob/acdd3493808e716825975b0f0208194bb2faf3c3/desktop_env/controllers/python.py#L872-L884)
controllers send `SCROLL` integer steps through `pyautogui.hscroll(dx)` and
then `pyautogui.vscroll(dy)`. The inspected PyAutoGUI 0.9.54
[source tarball](https://files.pythonhosted.org/packages/65/ff/cdae0a8c2118a0de74b6cf4cbcdcaf8fd25857e6c3f205ce4b1794b27814/PyAutoGUI-0.9.54.tar.gz),
SHA256 `dd1d29e8fd118941cb193f74df57e5c6ff8e9253b99c7b04f39cfc69f3ae04b2`,
uses one X11 wheel-button click per integer step in
`pyautogui/_pyautogui_x11.py` lines 42–65. No PyAutoGUI input was sent in this
probe.

The AUV binary was `0.0.28` from head
`20c430902081b7da4b7fa02a228d36fb398a698c`, SHA256
`76e421400bbdf4954bb08926bd7cc4a3e2ae4ab3c5aab3378792d390bc1abf4c`.
The disposable Docker Xvfb fixture used `auv-x11-validation:local`, image ID
`sha256:b08b6cefceb848bbe2e42818fd7d4bead6502b37716ce5489fe897a398897c99`
(Debian 12, aarch64, `DISPLAY=:99`). The independent Tk/`xev` receiver saw:

| AUV screen scroll | Run | Native event / receiver result |
| --- | --- | --- |
| `dx=0, dy=+240` | `01a10d9c-0ae4-7014-91bc-6dba7777f7b2` | Button 5 twice; Tk `yview` increased from about `0.4500` before the first event to `0.4599` after the second handler, including default Tk class scrolling between callbacks |
| `dx=0, dy=-120` | `01a10d9c-3e98-7014-8317-f382e9ee8151` | Button 4 once; Tk `yview` decreased from about `0.4658` to `0.4638` in the custom handler |
| `dx=+120, dy=0` | `01a10d9c-dbd9-73eb-9e29-76b22b2e9fcf` | Button 7 once |
| `dx=-120, dy=0` | `01a10d9d-149a-7376-95f5-cfda7e26d390` | Button 6 once |
| `dx=+120, dy=-240` | `01a10d9d-e56e-7156-8a8e-f6cbdce6c393` | Button 7 once, then Button 4 twice; horizontal-before-vertical order matches the pinned controller |

For equivalent X11 wheel events, the OSWorld adapter can map
`auv_dx = 120 * osworld_dx` and `auv_dy = -120 * osworld_dy` in AUV logical
pixels. This is a source-backed inference joined to native receiver evidence,
not a measurement of application-independent pixel displacement. The old
`auv-osworld-x11` Pod was not reused: on 2026-10-06 it was Evicted from
`neko-gpu-1` after an ephemeral-storage threshold breach. The task-owned
Docker fixture was removed; retained PVCs were untouched. A fresh official
V1/V2.1 guest test with the current binary is still required.

## Which OSWorld deployment model fits Kubernetes?

### Direct desktop container

The validated XFCE/Xorg Pod is ideal for driver development because it starts
quickly, shares sockets naturally, and exposes deterministic receiver state.
It is not benchmark-equivalent: it lacks the official application versions,
task assets, mocked sites, reset image, and evaluators.

The separate `xlang-ai/osworld_image` project documents a container-native
XFCE build that imports the qcow2 root filesystem, removes VM/systemd-heavy
pieces, runs DBus/Xvfb/XFCE/x11vnc/noVNC and the OSWorld server under
supervisor. This proves a full desktop can be containerized, but image parity
must be demonstrated per task; it is not automatically equivalent to the
released qcow2.

### Official Docker/QEMU runtime as a Pod

The official Docker provider launches `happysixd/osworld-docker`, mounts one
qcow2 at `/System.qcow2`, exposes ports `8006`, `5000`, `9222`, and `8080`, adds
`NET_ADMIN`, and passes `/dev/kvm` when present. If KVM is absent, it sets
`KVM=N` and uses slower software emulation. Kubernetes can express this
directly; Docker-in-Docker and the host Docker socket are unnecessary.

OSWorld-V2.1 pins:

- a 14,891,811,084-byte Ubuntu archive with SHA256
  `14b08aa7ba6c023ecb91d46de8df5de32af4d1d6bd75ea925519caf9677fc8b3`;
- runtime image
  `happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`;
- 108 task files plus pinned task, asset, website, and provider-image revisions.

The current OSWorld V1 Docker manager instead follows the mutable
`ubuntu_osworld/main/Ubuntu.qcow2.zip` URL and an unpinned runtime image. The
LFS pointer observed for this validation fixes the downloaded input at
12,273,896,463 bytes and SHA256
`b795b6cd4c69b252c1b4f10150a347795555032501b60fd031751ed09b896712`.
Record the resolved container image ID as part of every V1 run because the
upstream provider does not do so.

The released Linux binary built on Debian 13 was not suitable for the Ubuntu
22.04 guest because its glibc baseline was newer. A guest-compatible validation
binary was therefore linked against Jammy libraries and identified by SHA256
`7bc1f4256fa903d1bb660f73901d34c8aa2c0c85ed948473b9f8809b049cdc26`.
Jammy's packaged PipeWire `0.3.48` headers are too old for the workspace's
current `libspa` dependency, so this X11-only validation build used newer
PipeWire/SPA headers while retaining Jammy runtime libraries. This is an
isolated validation workaround, not evidence that the normal Linux release
artifact supports Ubuntu 22.04. A distributable artifact needs an explicit
minimum-glibc and optional-backend build policy.

Use a large RWO PVC for the archive and extracted qcow2. Keep the qcow2 on the
PVC, not container overlay: an earlier desktop Pod was evicted after consuming
about 21 GiB of ephemeral storage. The same rule applies to Cargo targets,
benchmark assets, browser profiles, and run evidence. Do not synchronously
write UI receiver state to remote iSCSI from an event callback; the fixture
became visibly unresponsive until its hot state moved to `emptyDir`.

Import also exposed a storage-throughput constraint. A single sequential
download was faster than reading the completed archive back from `tns-iscsi`
for SHA256 verification, and concurrent V1/V2 verification approximately
halved each reader's throughput. Importers should be serialized per storage
backend, and a production importer should calculate the digest while streaming
the download instead of performing a second full remote-volume read. Replacing
an importer Pod preserved partial bytes on its PVC, but detach/remount took
several minutes; retry policy should therefore live inside the importer rather
than rely on Pod churn.

For this cluster, separate cold and hot image tiers. Keep the immutable release
archive on `tns-iscsi` (digest-verified before production use), but extract the
runtime qcow2 to a `local-path` PVC bound to the KVM node. The V2 archive
contains an almost uncompressed 14,891,810,816-byte qcow2, so extracting it
back onto the same iSCSI volume performs another complete remote read and write
without reducing the payload. A node-local hot PVC avoids carrying that latency
into QEMU reads and episode startup. It is deliberately node-affine and not
highly available; the cold archive remains the recovery source and must be
digest-verified before production use.

OSWorld-V2 also requires self-hosted mocked websites and GitLab for the task
families that refer to them. A booted desktop and working AUV loop are necessary
but not sufficient for a comparable 108-task result.

### KubeVirt plus CDI

KubeVirt becomes useful when these requirements become primary:

- a declarative `VirtualMachine` lifecycle instead of a QEMU container;
- CDI HTTP/registry import into a `DataVolume`;
- PVC cloning or CSI volume snapshots for per-task reset;
- VM console, cloud-init, and guest lifecycle integration.

It is not required merely to gain KVM acceleration. The direct runtime Pod can
already mount `/dev/kvm`. Installing KubeVirt adds operator, virt-controller,
virt-api, virt-handler and webhook surfaces; CDI is a separate data-import
operator. The cluster did not have either CRD set during this validation.

KubeVirt's installation checks require `/dev/kvm` to exist and be accessible.
It can enable software emulation through
`spec.configuration.developerConfiguration.useEmulation`, but that is a slow
fallback rather than the target configuration.

Live node probe:

| Node | `/dev/kvm` | Implication |
| --- | --- | --- |
| `neko-gpu-1` | absent | direct Xorg fixture and HAMi are usable; accelerated QEMU/KubeVirt is not |
| `liet-gpu-1` | character device `10:232`, mode `0660` | preferred node for official QEMU runtime and any KubeVirt pilot |

`tns-iscsi` is RWO. A workload must be scheduled through a node selector rather
than `spec.nodeName` while its `WaitForFirstConsumer` PVC is unbound, otherwise
the scheduler cannot supply the selected-node annotation and the PVC remains
Pending.

## 2026-10-06 current-AUV action baseline in official guests

This rerun used the official V1 and V2.1 Ubuntu 22.04.3/Xorg guests on
`liet-gpu-1`, with runtime image
`happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`.
Both hot image PVCs were mounted read-only. The installed Ubuntu AUV 0.0.28
binary was built from `eee4e3c01e2f29f7582f834b6e2de87577cf7723` and
has SHA256 `c9ca279e3dfb9847915abbbe289dfd9326d7527f1e6d5ca6ff70133991f05e38`.
The `auv-osworld-evals` test executable has SHA256
`169344db871d53638ad068f5d6cf12d52750d2cc173b996c8dd1d4a69914b0c3`.
The later PR commit `6b002484` changes only TOML formatting, not Rust behavior.

In each guest, the installed AUV daemon's owner Unix socket passed the public
`ActionExecutor` Runner integration episode (V1 1/1 in 0.30s; V2.1 1/1 in
0.28s). An independent Tk receiver saw the rounded pointer move, Button1
down/move/up, Shift-held `A`, Button5 scrolling with `yview` increasing from
0 to 0.005882, right click, and error-path hold release. AUV CLI probes in
both guests additionally produced raw `xev` Button6/7 horizontal wheel events,
Button2, double/triple clicks, a 400 ms Button1-held drag with at least 24
motion samples, `abc猫Z界` text, F13, Ctrl+Shift+F13, 500 ms overlapping
Ctrl/Shift holds, and reverse vertical scroll returning Tk `yview` to 0.
These extra CLI probes validate the installed AUV primitives, not every
`ActionExecutor` variant end-to-end.

| Guest | AUV display/capture evidence | Independent receiver files |
| --- | --- | --- |
| V1 | `display.list` Run `01a10dc3-82e3-71ee-b71d-aa26bbd420f2`; capture Run `01a10dc3-9742-7581-8127-095913b28bb0`, PNG SHA256 `82d06b2f57c1d6046ceae8e754ae4682a4825dcd68fc77e37ed7e1ab0b16cc9c` | Tk SHA256 `4506f932a9ecbb85942540bb334e07ff69b6f9d27d4bbf8e8d1450617f873ad1`; xev SHA256 `2072f603516aa8307d561a5171c84be2cde2bdcd3e7f3f136c8cc9740ae6a479` |
| V2.1 | `display.list` Run `01a10dcb-ee4a-7108-a4a5-588a429f84e2`; capture Run `01a10dcc-0591-7048-a370-58db408f980f`, PNG SHA256 `47e63ee098417f233d99ce2793cb617fe5f8ff3fa01eea3cf223e2481c927886` | Tk SHA256 `e699c3bf087f594347e56106e7c2895efdf6dea87167a3bcc3b2204cc31897c3`; xev SHA256 `b37d359fb7007f05bfc05f1debaeb13295b2cc07a3b2e57b4e8be6a60dfd4d70` |

The paired Mac→V2.1 path used `serve --listen` with a token created through
the guest owner Unix socket. The external AUV observed `Virtual-1` 1920×1080
(`display.list` Run `4bd74939-472e-dfe7-e5e3-d59de641906c`), captured
the guest display (Run `40f9f437-0b71-38be-f2c1-fc567666773c`, PNG SHA256
`815e81d570a9c4822467daec09bc8e34e67bab607f851de71f58822a130db9a4`),
and sent `input.clickPoint 322 234` (Run
`8d6513f1-c1f3-06a1-8ccc-e34923b0913a`). The guest Tk receiver recorded
Button1 press/release at (322,234). This proves remote observation and input,
but the remote `ActionExecutor` integration episode itself was not rerun.

Raw local evidence is under `/tmp/auv-osworld-gate-20261006/` on the task
host, including `v1-{tk-receiver.jsonl,xev.log,auv-capture.png}` and the
corresponding `v2-` files. This scratch directory is not a durable CI artifact.
All task-owned build/VM/proxy/cleanup Pods, Services, port-forward, local
paired profile, and task-owned build directories were removed after capture;
the retained PVCs were not changed. The old `auv-osworld-x11` Pod is Evicted
and remains `ContainerStatusUnknown`; it must not be cited as current live
evidence.

This is an action-level baseline, not an OSWorld evaluator result, completion
rate, GPU/DRA test, or full paired-runner test. In particular, a checked
`InputActionResult` does not replace application-state or task-evaluator
verification. A batch agent/harness run remains the next separate gate.

### Complete adapter action matrix on isolated Xvfb

The later test-only `evals/osworld/tests/xorg_actions.rs` exercises the public
`ActionExecutor` across every currently supported structured GUI action family.
On the task-owned Docker/Xvfb display `:99`, the installed AUV daemon and
independent Tk receiver verified pointer rounding, button/count clicks
(including no-coordinate click at the current pointer), held move/drag,
ASCII/Unicode text, F13, single and overlapping modifier holds, hotkey,
control signals that emit no GUI input, and error/finish cleanup. A separate
raw `xev` window observed the mixed-axis scroll ButtonPress sequences
`[7,7,5,5,5]` and `[6,4]`; Tk's generic binding folds horizontal wheel
events into its vertical binding on this image, so its log is not used for
horizontal-wheel claims.

The final receiver assertion for no-coordinate click saw Button1 press and
release at (640,440) after `MOVE_TO`; the updated live test passed 1/1 in
2.55 seconds. Source base was `41cc965f` plus the test-only diff; the test
file SHA256 was `172d9563dffb06b22268d66fe17a747fe0c810ba303801dd20501db16046ef08`,
and the Tk receiver script SHA256 was
`00f123e99d8b3486ec5fedba0a4587ac4f3ab6dce6f06aac6a843bef0101e344`.
The fixture image ID was
`sha256:b08b6cefceb848bbe2e42818fd7d4bead6502b37716ce5489fe897a398897c99`
and installed Linux AUV binary SHA256 was
`b3587932771b2c50316fd650e3415bd6f07d34dcc1a9e1f6743ea458231b2d23`.
The task-owned container was removed afterward. This confirms the adapter's
local X11 action delivery, not an official guest, evaluator, or task score.

### Complete adapter action matrix in official V1 and V2.1 guests

The same test-only matrix was then built for Ubuntu 22.04 from PR commit
`4b12d87d` and run through the installed AUV daemon's owner Unix socket in
each fresh official guest. The test executable SHA256 was
`71738202b94f59c7430a643f9e507905adad1b4e42f5d2c23af455f96bf7267c`;
the installed AUV binary SHA256 was
`c9ca279e3dfb9847915abbbe289dfd9326d7527f1e6d5ca6ff70133991f05e38`.
Both guests ran Ubuntu 22.04.3, Xorg `Virtual-1` at 1920×1080, under the
pinned runtime image digest above. Their hot qcow2 PVCs were read-only.
Independent Tk and `xev` receivers, not AUV's delivery result alone, checked
the actual X11 events. The matrix passed 1/1 in V1 (2.65 s) and 1/1 in V2.1
(2.71 s). V1 `display.list` Run was
`01a10df4-0b6b-7394-8c4b-f6db557524bd`; V2.1 Run was
`01a10df7-46b7-7013-8c77-025f1aa0bbcc`.

The structured test results are
`/tmp/auv-osworld-gate-20261006/v1-matrix-result.json` (SHA256
`f55f9f720598457ee558cf3719c3356531718ec9a5950d3b4da894d2a89199a2`)
and `v2-matrix-result.json` (SHA256
`93322d1d946c372bc35244c003197af865cf233e1666df0bd04d2db6e549a050`).
These files are task-host scratch evidence, not durable CI artifacts. This
closes the guest-local **adapter delivery** gate. It does not establish an
evaluator score, a full remote-paired action episode, or task completion.
The task-owned build, V1/V2.1 runtime, proxy, and cleanup Pods; two task-owned
Services; port-forward; temporary manifests; and 1.9 GiB of task-owned build
directories were removed afterward. The retained PVCs remained Bound; the
pre-existing Evicted `auv-osworld-x11` Pod was untouched.

### V2.1 Task099 setup/evaluator negative control

A separate task-owned fresh overlay on `liet-gpu-1` used the pinned V2.1 code
checkout `3d778a3c9a34a079316f70df023b166700445792`, the same runtime
image digest, and a writable `/boot.qcow2` backed by the read-only retained
`osworld-v2-hot/System.qcow2`. The release-matched `task_099.py` SHA256
`58c460fdfecf518f64714fdc21933b60818d8cf28ec02f9fa15a10e56ef02e32`
matched the task manifest. Its sole gated asset, `task_099/my_image.png`,
matched SHA256 `6c99998e7275e2132c6e27d01f212eaabfbf1c10b0e9405d11aa125eb94a16c1`
on both the task host and guest Desktop.

The exact pinned task `setup()` and `evaluate()` methods and original
`get_vm_file` getter ran through a task-owned minimal HTTP file-transport
adapter, **not** the full upstream runner. Setup reported `File Uploaded:
1851281 bytes`. With no `position.txt` submitted, evaluation correctly
returned `0.0`; the result is
`/tmp/auv-task099-infra-20261006-evaluator.json` (SHA256
`58588ffa0ad48f29ee6d0caf3ca4dc8530b2019d967f4a5888a0d5750ac45e10`).
The transport adapter SHA256 was
`c706c1b811ec31f07a50e379f699142c439ff723f9693543ba4938b2ffab1573`.

Installed AUV 0.0.27 (binary SHA256
`7bc1f4256fa903d1bb660f73901d34c8aa2c0c85ed948473b9f8809b049cdc26`)
listed `Virtual-1` 1920×1080 (Run
`01a10e01-4459-70d2-b6d2-4b80b569ce00`). AUV click, Ctrl+L, text, and
Return Runs opened Google Maps in Chrome; final AUV capture Run
`01a10e03-b6b8-76e2-af1d-fe67ed9d458a` produced
`/tmp/auv-task099-maps-20261006.png` (SHA256
`ce78f2864378296828c46370f574b32b1b14c2a764846f83a0468920d32d08f8`),
visually showing loaded map tiles and search UI. The AUV source commit for
this older Jammy validation binary cannot be independently established; do
**not** attribute this trial to current PR head. No geolocation agent ran,
no coordinates were submitted, and `0.0` is a negative control, not a task
performance result. The task-owned VM Pod, proxy Pod, Service, and
port-forward were removed; `osworld-v2-hot` remained Bound.

### V2.1 Task099 blinded agent attempt on current PR head

For a separate fresh overlay, Ubuntu 22.04 built `auv-cli` from exact PR
source commit `25e2320570a72d3b9580451ea2917a9e03fa6b95`. The source archive
SHA256 was `c0d3e75daefce012f9d202d931ff22ecb89a0f74971ec047c01280146fbe8e3a`;
the installed AUV 0.0.28 binary SHA256 was
`2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`.
The build needed newer libspa/PipeWire headers on the task-owned build PVC
while retaining Jammy runtime libraries; an initial build Pod on
`neko-gpu-1` was Evicted for node ephemeral-storage pressure. The successful
build used a task-owned 40 GiB PVC on `liet-gpu-1`; its Pod/PVC were removed
after extracting the binary. No repository source was changed for the build.

The pinned Task099 `setup()` placed the same image in the guest, and its SHA
matched the asset above. AUV `display.list` Run
`01a10e1b-3b6a-754e-ba78-65c9843308df` reported Xorg `Virtual-1`
1920×1080; capture Run `01a10e1b-3bc2-7202-b084-bfeb6ea89cf0` succeeded.
An authenticated, paired AUV-only port-forward was given to a fresh subagent
that did not inherit this task's history, task source, evaluator, or answer.
The subagent could open the desktop image and use Google Maps/Street View
through AUV screenshots and input. It spent an extended exploratory period
on location confirmation and was stopped at a disclosed ad-hoc limit before
writing an answer. No evaluator feedback was given during the attempt.

After the subagent stopped, exact pinned `Task099.evaluate()` and the original
`get_vm_file` getter ran through a minimal file-transport adapter, **not**
the full upstream runner. The evaluator found no
`/home/user/Desktop/position.txt` (HTTP 404), returning total score `0.0`
and distance partial score `0.0` with weight `1.0`. The result is
`/tmp/auv-task099-blind-eval-20261006.json` (SHA256
`496b15f9f7fa545ffc6fb0cde8372403015f0b59bd836e9cc6ad35ca0ab7af34`);
the evaluation adapter SHA256 was
`967b0ec0d0b28c9696f14665917412ccbd8f608c4d3dd5c14428f9b37c73c8e6`.
This is **one incomplete, time-bounded agent attempt**, not an OSWorld-V2.1
completion rate. The AUV connection, capture, and GUI navigation worked;
the missing answer file is not evidence of an X11 input-delivery failure.
The task-owned VM Pod, proxy Pod, Service, and both local port-forwards were
removed and verified absent; the retained hot PVC remained Bound.

## Recommended harness boundary

Treat infrastructure, benchmark control, and computer use as different
authorities:

```text
Kubernetes lifecycle
  -> image/PVC/network readiness
OSWorld setup/evaluator control plane
  -> reset, task setup, task-state evaluation
AUV harness
  -> display.capture -> model/human decision -> typed AUV input -> Run artifacts
```

The harness should record one AUV Run per benchmark episode and retain:

- pinned OSWorld release and task identifier;
- qcow2 and runtime-image digests;
- Device ID, driver descriptor, display geometry, and topology name;
- every typed action request/result and screenshot artifact;
- setup/evaluator results separately from input delivery;
- environment reset identity and terminal reason (`DONE`, `FAIL`, timeout).

Do not interpret `InputActionResult.verified = false` as a task failure. It
means delivery succeeded without semantic verification. OSWorld's evaluator is
the separate semantic authority.

## Bounded two-episode V1 AUV pilot (2026-10-06)

This exploratory pilot used pinned OSWorld V1 revision `b138d348`, the
official Xorg/KVM guest image on `liet-gpu-1`, and a fresh writable overlay of
the retained read-only `osworld-v1-hot` image for **each** task. The runtime
image digest began `0e6497a929`; guest AUV was built from source `25e23205`
with binary SHA256
`2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`.
Each task had a fixed 10-minute AUV action window, independent of boot, setup,
and evaluation time. The scheduled denominator was two, including the timed-out
episode.

| Episode | Topology and action window (UTC) | AUV evidence | Evaluator input and result |
| --- | --- | --- | --- |
| VLC `5ac2891a-eacd-4954-b339-98abba077adb` (task JSON SHA256 `4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da`) | Guest-local AUV via owner Unix socket and temporary key-only SSH for the harness; 2026-10-05 23:04:30–23:14:30, full window elapsed | Capture Run `01a10e4f-7838-76ec-9474-7e088828548f`, PNG SHA256 `b2bf00c08ef74d4ce904640a9af322462cc6eca0f40e9fd21a2ca93095f74519`; AUV Ctrl+P and clicks reached VLC, but Advanced Preferences partially redrew over the old pane | `vlcrc` SHA256 `9b24563be2e95f0df1f958b5f57126e6ed6206bf08ca978d74de645ebbfb32f2` still contained `play-and-exit=1`; exact pinned `check_play_and_exit` returned `0` |
| Chrome `2ad9387a-65d8-4e33-ad5b-7580065a27ca` (task JSON SHA256 `4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c`) | Paired Mac AUV → guest Device `25d29d1e1aae`; window 2026-10-05 23:37:17–23:47:17, action completed around 23:40 | Pre-action capture Run `5d43fdf3-64e6-8beb-cf72-14d673b4257d`; saved-folder capture Run `f695f9bd-cd6a-908c-f85a-9d5560dcdca9` (PNG SHA256 `520d39a7d292cdc6f87cc7271e87e161e357e880dd2e3223a5b4defd06526c2a`); post-restart capture Run `d114301d-217c-8410-adcc-5054a0c2f7d8` (PNG SHA256 `0a9c3eecd46e662da196edfa63ab9cb906a44d356ae35b03d38ec12dde001ed8`) | After official `pkill`/relaunch/sleep postconfig, Bookmarks SHA256 `59feac785ced9e7ba5e79fe6ec96ed558e1e7bfad7d9165296645aedc59c6862`; exact pinned `is_expected_bookmarks` returned `1.0` for `Favorites` |

The Chrome Mac-side AUV binary SHA256 was
`cf9485c4a2ec0fbf14c0fa6f874ef77decba00f8c61ae704ec67b77915a3c08a`.
Its paired `display.list` Run `caa49f3f-3ba9-5b56-aece-89a792cf2409`
reported `Virtual-1` at 1920×1080. The evaluator source was audited before
selection: neither task's reachable setup or evaluation path calls a
PyAutoGUI input function. The upstream helper imports PyAutoGUI while
resolving a guest path, but this pilot used the official setup/postconfig,
getter-equivalent `/execute` path lookup and `/file` retrieval, then extracted
and invoked the exact pinned metric functions. OSWorld `/execute` was **not**
used to deliver AUV GUI input. The raw metric outputs were observed in the
pilot tool session but were not retained as standalone JSON files; the
evaluator inputs remain in task-local scratch under
`/tmp/auv-v1-pilot-vlc-evaluator-input-20261006` and
`/tmp/auv-v1-pilot-chrome-bookmarks-20261006.json`.
An independent read of those retained inputs found
`play-and-exit=1` at `vlcrc` line 4073 and exactly one bookmark-bar folder,
`Favorites`, under `roots.bookmark_bar.children`. The pinned Chrome getter
returns the `roots` object to the metric, matching that input shape.

These are **metric-function results**, not runs of the full
`DesktopEnv.evaluate()` runner. The selected-slice mean is `(0 + 1.0)/2 = 0.5`
only as a two-task bookkeeping value; it is neither an official OSWorld score
nor an estimate of AUV agent completion rate. VLC's observed failure layer
was agent navigation/UI redraw within the fixed action window, not AUV
transport. Chrome showed the requested folder before and after postconfig.

In namespace `auv-x11-hami-test`, the task-owned VMs
`auv-v1-pilot-vlc-20261006` and `auv-v1-pilot-chrome-20261006`, their
same-named Services, `-proxy` Pods, and port-forwards were deleted and
verified absent. The temporary Chrome pairing profile and VLC SSH key were
removed. The retained hot PVC remained Bound. A durable scheduler, retained
per-episode raw evaluator JSON, and full upstream runner integration remain
next gates.

### Chrome evaluator-only method-body control pair

A subsequent Chrome-only bridge in `evals/osworld/v1_evaluator.py` executes
the **pinned original method bodies** for task binding, setup, postconfig,
getter, and `DesktopEnv.evaluate()` against an externally managed QEMU guest.
It avoids importing V1's entire provider/evaluator dependency tree by selecting
only the audited methods from a clean V1 checkout. It does **not** instantiate
the upstream Docker provider, call `DesktopEnv.reset()`/`step()`, or represent
a full official runner. The bridge allowlists only Chrome task
`2ad9387a-65d8-4e33-ad5b-7580065a27ca` at task SHA256
`4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c`.
VLC is deliberately not enabled in this method-body path yet.

On `liet-gpu-1`, two **independent** fresh V1 read-only-base overlays used
QEMU runtime image digest
`sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`.
Both phases ran with `PYTHONDONTWRITEBYTECODE=1` and a 180-second process
timeout. No GUI input was sent in the negative control:

| Control | UTC phase times, 2026-10-06 | Exact bridge score | AUV evidence |
| --- | --- | --- | --- |
| No-action negative, `osworld-v1-eval-probe-1006` | prepare 00:00:50–00:00:51; evaluate 00:00:55–00:00:59 | `0.0` | None; intentionally no GUI input |
| AUV-only positive, `osworld-v1-eval-pos-1006` | prepare 00:06:09–00:06:10; AUV action to about 00:07:37 (under 10 minutes); evaluate 00:07:41–00:07:46 | `1.0` | Paired Mac AUV → guest installed AUV 0.0.28 (guest SHA256 `2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`); final capture Run `81a8847d-d382-0999-28b6-416a15e421a8`, PNG SHA256 `c252c8bd7659c13c02760d98504ba1b8baa74a67c2b13839ccf166aa954d2dda` visibly shows `Favorites` |

The positive control used AUV for Ctrl+Shift+B, bookmark-bar context menu,
folder creation, text entry, Save, and capture. Setup/evaluation used only
their control-plane endpoints; neither OSWorld `/execute` nor PyAutoGUI sent
AUV GUI input. The original bridge stdout reported `"score": 0.0` and
`"score": 1.0` respectively. These are **evaluator-only method-body control
results**, not full `DesktopEnv`/provider benchmark scores or a general
completion rate. The bridge's local pinned-source contract tests passed 2/2.
Its episode marker checks task hash and endpoint but cannot prove a reused
port-forward still reaches the same guest; a durable scheduler must pin the
Pod UID across phases. Both temporary VM/Service/proxy sets and Mac pairing
profile were removed and verified absent; the V1 image/hot PVCs stayed Bound.
An Alpine proxy initially failed to fetch `apk` from its mirror, so the live
probe used a cached Python slim image's stdlib TCP proxy. The guest needed
`libtesseract4`, `liblept5`, and English Tesseract data installed before this
path ran. These are infrastructure prerequisites, not hidden task actions.

### Bounded runner's first V1 live gate: boot failure, no task attempt

On 2026-10-06, the new local-tested batch runner and experimental Kubernetes
adapter scheduled one Chrome capture-only negative control on `liet-gpu-1`.
Before boot, a task-owned Pod read the V1 hot PVC read-only: `System.qcow2` was
24,460,197,888 bytes with SHA256
`6bf667a852b3c307f61d9f09c42559351f45e0607e428b4997becf534cf4d313`.
That measurement Pod was UID-precondition deleted; the retained PVC/PV UIDs
and Bound states were unchanged.

The runner ledger fixed denominator `1` before execution. Boot created the
task-owned runtime Pod, proxy Pod, and Service, then failed at its explicit
overlay check: the live QEMU command included `-enable-kvm` but booted
`-hda /boot.qcow2`, without `-snapshot` or `/System.qcow2`. The adapter had
assumed a QEMU `-snapshot` overlay and correctly refused to infer a fresh
guest from this different command. No install, setup, AUV action, evaluator,
AUV Run, or score occurred. This is an infrastructure validation failure,
**not** an agent result. The [ledger](/private/tmp/auv-osworld-v1-negative-config.pYIvsQ/live-20261006-0205/ledger.json)
has SHA256 `6c2aaffe85cec6363230125a0abdd5beaef2898e3fcc10fe8cb8a50174078627`;
its boot and reset stderr digests are `e631586c7c1a05b8827cd974402ef67e688a4b9bbdbcf09dc4db5893abc4ff9d`
and `09c30d42c2ce6d6c1a4a0a451f08e06b3e1ff8af0ad83f487d65df783fe4fa`.

Automatic reset sent a UID-preconditioned DELETE for the proxy Pod, but its
30-second disappearance check timed out just before the proxy vanished.
After checking ownership labels and UIDs against the task journal, the operator
used the same UID-preconditioned API path to remove the remaining runtime Pod
and Service. All three are now absent; the V1 hot PVC/PV UIDs remain Bound.
The reset wait policy has since been widened with a regression test, but has
not yet been rechecked in a live episode. The next slice must verify the
image's actual backing-file overlay mechanism before another attempt. Do not
reuse this failed episode or report it as a capture-only evaluator control.

A second, independently named episode at 02:27:20–02:30:00 UTC passed boot
in 116.7 seconds. The [boot evidence](/private/tmp/auv-osworld-v1-negative-live2.wn8vlp/run/chrome-capture-only-b/boot.stdout)
records QEMU PID 7 using `-enable-kvm -hda /boot.qcow2`, live `qemu-img`
backing metadata pointing to `/System.qcow2`, the measured base SHA256,
container-root `overlay` filesystem, an open QEMU FD to the boot file, and
stable Pod/container identity. This supports a fresh backing-file overlay for
that guest, not QEMU `-snapshot`. Install then failed after the guest returned
a successful plain-text `/setup/launch` response: the adapter incorrectly
called `json.load` on it. The pinned upstream handler explicitly returns
`"... launched successfully"`; a regression test now reproduces and fixes
that parsing error. No setup/action/evaluation, AUV Run, or score occurred.
Reset completed in 34.1 seconds with UID-preconditioned deletion of all three
resources; the retained hot PVC/PV stayed Bound and the pairing profile was
absent. The [second ledger](/private/tmp/auv-osworld-v1-negative-live2.wn8vlp/run/ledger.json)
has SHA256 `7d0c6b66633728e607a1156f700d240851da4f9795133664994c9fc3484f8fc2`.
The corrected launch-response path has not yet been rechecked live.

A third independently named episode, `osw-v1-neg-1006c`, ran at
02:39:22–02:42:05 UTC. Boot and the measured backing-file checks passed in
120.3 seconds. Installation got past `/setup/launch` but stopped after 8.7
seconds because the pairing command's stdout was not exactly one token line.
The original adapter did not record the guest command's `returncode` or safe
stdout shape, so this run cannot distinguish a failed command from an empty or
multiline success response. The pinned OSWorld `/setup/execute` can return
HTTP 200 with `status=success` for a nonzero guest exit. Regression tests now
require a zero exit and record only stderr/stdout hashes, byte counts, and
token-line count on failure; bearer material is never printed. The
[third ledger](/private/tmp/auv-osworld-v1-negative-live3.mcTGGX/run/ledger.json)
records no AUV Run or score. Reset completed in 34.1 seconds, removing the
task-owned VM Pod, proxy Pod, and Service by UID preconditions; the V1 hot
PVC/PV remained Bound and the pairing profile was absent. The revised
pairing diagnostics have not yet been rechecked live.

A fourth independent episode, `osw-v1-neg-1006d`, used the exit-code-aware
adapter at 02:53:46–02:56:20 UTC. Boot and the overlay audit passed in 113.1
seconds. Installation failed in 6.0 seconds at `/home/user/auv --version`,
before daemon launch or pairing: the pinned setup endpoint reported HTTP 200
and `status=success`, but the child exited 127. The safe stderr evidence was
131 bytes with SHA256
`1b61c7ad3b5ebbe53a025ca9f504692994779dabb87fbe8fe2747b8d4681f877`.
The local ELF requires `libtesseract.so.4`; the exact dynamic-loader message
for that missing library has the same length and SHA256. This identifies the
guest prerequisite without exposing any token or raw command output. The
[fourth ledger](/private/tmp/auv-osworld-v1-negative-live4.K6cjrg/run/ledger.json)
has no AUV Run or evaluator score. Reset completed in 34.9 seconds, all three
task-owned resources were absent on an independent check, and the hot PVC/PV
UIDs and Bound status were unchanged. This does not revalidate installation
after adding the missing guest library.

A fifth independent episode, `osw-v1-neg-1006e`, ran at
03:04:54–03:07:50 UTC. Boot, the fixed apt prerequisite, guest AUV
`--version`, pairing, and official Chrome setup all passed. The paired-host
capture then failed in 3.0 seconds before reaching the guest: the adapter
passed the canonical Device ID to CLI `--device`, which selects an exact
Device **name**, not an ID. The host reported `Device selection does not match
the local daemon or a paired Device profile`. The original evaluator returned
`0.0` on the unchanged desktop, but AUV evidence was absent: no Run ID or
PNG. This is an evaluator-only negative observation, **not** a complete
capture control or an agent result. The [fifth ledger](/private/tmp/auv-osworld-v1-negative-live5.8x8Fxy/run/ledger.json)
records every phase and its status. Reset succeeded; the three task-owned
resources were independently absent, the pairing profile was removed, and
the hot PVC/PV retained their original UIDs and Bound status. A test-first
adapter fix now uses `--device-id`; that change has not been rechecked live.

A sixth independent episode, `osw-v1-neg-1006f`, completed at
03:14:23–03:17:25 UTC on the corrected adapter. The fixed denominator was
one; boot, install, setup, paired-AUV capture, pinned evaluator, and UID-safe
reset all finished `ok`, with no failure layers. AUV Run
`2b84ead3-e774-bb9d-fb7a-33dbb3ec27aa` produced a 1920×1080 Chrome
new-tab [PNG](/private/tmp/auv-osworld-v1-negative-live6.xaK2a5/run/chrome-capture-only-f/final-screenshot.png)
with independently verified SHA256
`c1489b4d7f7311c07554954ffd7e1fcc301486b631f373a07e4e1e64428b31cd`.
The exact pinned evaluator returned raw `0.0`, as expected when the only AUV
action is a screenshot and no task-solving GUI input is sent. The
[completed ledger](/private/tmp/auv-osworld-v1-negative-live6.xaK2a5/run/ledger.json)
records verified AUV evidence, the raw score, all phase times, and cleanup.
After reset, the three task-owned resources were independently absent, the
pairing profile was removed, and the V1 hot PVC/PV retained their original
UIDs and Bound status. This validates one capture-only infrastructure path,
not an agent attempt, task completion rate, V2.1 batch, or full upstream
provider integration.

### Pinned VLC capture-only Kubernetes control

An independent V1 VLC `play-and-exit` episode, `osw-v1-vlc-neg-1006g`, ran
at 03:53:02–03:56:02 UTC. It selected task
`5ac2891a-eacd-4954-b339-98abba077adb` and its pinned JSON SHA256
`4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da`.
On a fresh verified overlay, all six phases passed: boot, installed/paired
AUV, pinned VLC setup (including guest `play-and-exit=1` postcondition),
paired-AUV capture, the original pinned method-body evaluator, and UID-safe
reset. The [ledger](/private/tmp/auv-osworld-v1-vlc-negative-live1.SXC05i/run/ledger.json)
records fixed denominator one, no failure layers, and verified AUV Run
`cf47a4bb-4d8f-99d2-080c-af809a0fef04`. Its [PNG](/private/tmp/auv-osworld-v1-vlc-negative-live1.SXC05i/run/vlc-capture-only-g/final-screenshot.png)
shows the 1920×1080 Ubuntu desktop with the VLC main window; independently
measured SHA256 is
`8caf7e9a402e266167beb8ffc6e525ee471d98e00f9cad376330da96dd71ac95`.
The raw evaluator score was `0.0`, expected because no task-solving GUI input
was sent. After reset, the three task-owned resources were independently
absent, the pairing profile and local forwards were gone, and the V1 hot
PVC/PV retained their original UIDs and Bound state. This is one
capture-only infrastructure control, not a scripted AUV solution, a blinded
agent attempt, or a two-task completion rate.

### First current-head typed-action gate stopped before action

The first paired-remote trial of the new foreground `auv-osworld-action`
entry used a fresh V1 Chrome overlay and a frozen `CLICK` then `DONE` plan.
Its [ledger](/private/tmp/auv-osworld-v1-typed-live.4XQ8sS/output/ledger.json)
records boot and guest AUV pairing as `ok`, but setup as `exit_failed`:
the manifest's `/opt/homebrew/opt/python@3.14/bin/python3.14` lacked the
`requests` package imported by the pinned evaluator bridge. The
[setup stderr](/private/tmp/auv-osworld-v1-typed-live.4XQ8sS/output/chrome-typed-click-h/setup.stderr)
contains `ModuleNotFoundError: No module named 'requests'`. Action and
evaluation did not run; there is no typed input result, AUV Run, PNG, or
score. This is a host harness dependency failure, **not** a failed AUV GUI
delivery. Reset succeeded and independently confirmed absence of the three
UID-matched task resources and paired profile; the V1 hot PVC/PV remained
Bound with unchanged UIDs. Do not use this trial as action-capability evidence.

The next independent fresh Chrome episode, `osw-v1-typed-1006i`, used the
same frozen `CLICK(1885,87) → DONE` plan with a Python interpreter that passed
the new `requests` preflight. Its [ledger](/private/tmp/auv-osworld-v1-typed-live2.efmFXQ/output/ledger.json)
records all six phases as `ok`, one completed episode, and no failure layers.
The foreground action entry created AUV Run
`0c007a310e9d8a5589f3d7e92b879a08`; its original
[`InputActionResult`](/private/tmp/auv-osworld-v1-typed-live2.efmFXQ/output/chrome-typed-click-i/input-action-results.json)
records one successful `foreground_system_events` click attempt with
`verified=false` and `mouse_disturbance=foreground`, followed by `DONE` with
no delivery. The same Runner's [final PNG](/private/tmp/auv-osworld-v1-typed-live2.efmFXQ/output/chrome-typed-click-i/final-screenshot.png)
has independently checked SHA256
`62f98cefbe12ba7007eed7259e29f8983e67eb80e2479b407ae06ce199ffbe4f`;
the action sidecar and terminal stdout agree. The capture shows a Chrome
update popover, but no before/after receiver proves the click caused it.
The pinned evaluator returned raw `0.0`: the fixed click did not solve the
Chrome task. UID-safe reset removed all three task-owned resources, removed
the pairing profile, and preserved the hot PVC/PV UIDs and Bound state. This
is one paired-remote typed-action delivery gate, **not** a multi-task batch,
semantic success claim, or agent completion rate.

### Frozen two-episode paired typed-action infrastructure batch

Batch `osw-v1-typed-batch-1006j` fixed its denominator at two before boot:
the pinned Chrome and VLC tasks, each with a separate fresh V1 qcow2
backing-file overlay, its own Pod/Service/proxy and 600-second action budget.
The [frozen manifest](/private/tmp/auv-osworld-v1-typed-batch.ojy4vd/manifest.json)
has SHA256 `8793ff9bc44fc6d1fa3002bb6cb6228a835dd0db43174e8776f19b1b1a78c384`.
It used the original infrastructure template byte hashes
`f08e691ac78e8ddec9f1562a0d5401a57a129d75d49797c75a73b5ee171ad015`
and `61e30c96b3936ba279fa2b290a86211108bab51a3eb5bb6b39e198d919b46b54`.
Both templates contain only `MOVE_TO → DONE`; they are not solution scripts.
After the run, JSON whitespace was reformatted for repository lint, changing
the checked-in template byte hashes without changing parsed actions. Use the
frozen manifest and its original hashes when interpreting this episode.

The [ledger](/private/tmp/auv-osworld-v1-typed-batch.ojy4vd/output/ledger.json)
has independently checked SHA256
`0896da2eac575f20f25a511b8dc2172831d71adc0b0729cee78947e6c4863842`.
Both episodes were `completed`, every boot/install/setup/action/evaluate/reset
phase was `ok`, and neither had a failure layer. Chrome AUV Run
`631e759bd609be0c0770a23366345fc1` produced a [PNG](/private/tmp/auv-osworld-v1-typed-batch.ojy4vd/output/chrome-move-j/final-screenshot.png)
with SHA256 `24255611ab54b9ec9dd07c91e6e30419845947c9443b53b50b6d3fa5620804d2`;
VLC Run `648ee0e9eb9c385558b10c292b641207` produced a [PNG](/private/tmp/auv-osworld-v1-typed-batch.ojy4vd/output/vlc-move-j/final-screenshot.png)
with SHA256 `8a3926c73a785da2991993d7e0d8eb0d247b61dcb040a3395912dfc1eeab5cc0`.
Each original `InputActionResult` contains one successful
`foreground_system_events` move attempt with `verified=false`, then `DONE`
with no delivery. The sidecars and screenshot bytes passed ledger validation.
The pinned evaluator returned raw `0.0` for Chrome and `0.0` for VLC, as
expected for non-solving moves. These are **not** two agent failures, a
computer-use completion rate, or evidence that the task-solving actions are
missing from AUV. Both UID-safe resets removed their three task-owned
resources, and independent checks found no remaining batch resources; the
V1 hot PVC/PV remained Bound with unchanged UIDs. This normal-completion
trial did not verify hard-timeout cancellation or held-input release.

### Static Chrome Favorites typed-action pilot did not reproduce the positive

A later one-episode Chrome trial froze the exact AUV action sequence recovered
from the earlier evaluator-positive manual session: Ctrl+Shift+B, right-click
the bookmark bar, choose `Add folder...`, type `Favorites`, click Save, then
`DONE`. The frozen action JSON SHA256 was
`044da62d16d371ef839440f5d4799ebf0e6d85708367afed33c85d37eac054a6`;
the manifest SHA256 was
`1fe69eadc8c880aae73679bffb8c60b541a82fd3a9326bd194c1cdc8b1c0ba7e`.
An AUV-only
pre-action [capture](/private/tmp/auv-osworld-v1-positive-typed.sSX5kB/output/chrome-positive-static-k/pre-action.png)
with SHA256 `74fdf4c97d899723b18bba8fd4120004308b459990b04d03c87ca19fb158e857`
was checked for the expected 1920×1080 Chrome initial state without changing
the plan. The [ledger](/private/tmp/auv-osworld-v1-positive-typed.sSX5kB/output/ledger.json)
has SHA256 `0279ab30bf82710bf00c2af122bad7911226077f0c1dd68988b33c03b4ec3c42`:
all six phases completed, no failure layer, and UID-safe reset preserved the
hot PVC/PV. AUV Run `e3e36fa0d76ec629b03bf422d7ee6df7` recorded five
successful `foreground_system_events` delivery attempts, each with
`verified=false`; `DONE` had no input delivery. Its same-Runner
[final screenshot](/private/tmp/auv-osworld-v1-positive-typed.sSX5kB/output/chrome-positive-static-k/final-screenshot.png)
has SHA256 `243bd44472ae566bfa07fc46326a70aa18a9e3c6cee1c953ac865de9316f80ce`.
It still shows the `New folder` dialog with `Favorites` entered, the bookmark
bar selected, and Save visible. The original pinned evaluator returned
`0.0`. Thus the first four actions had visible UI effect, while the last
click was not semantically confirmed before capture/evaluation. The plan had
no intermediate observations or post-click delay; whether timing or delivery
caused the Save failure remains unresolved. This is one failed static-script
pilot, **not** evidence that AUV lacks a save-click capability or an agent
completion-rate observation. No coordinates were changed or retried in this
episode.

A second independent Chrome replay kept the exact same action-array bytes
(SHA256 `cd4e301159692edafeb5e535642ddc860b0413bea2d9c953191d410122c7ea4e`)
and changed only the new plan field `final_settle_ms` to `2000`. The
[ledger](/private/tmp/auv-osworld-v1-positive-settle.YmIGAE/output/ledger.json)
has SHA256 `216a509bc4b17b7aaa5a98ff13e3b18945d9c2ae10c88f366ecf9e85065eefdc`.
All six phases and UID-safe reset again succeeded. Run
`b000a3f2d70588629345d54e84744628` recorded all five typed deliveries as
successful system-event attempts with `verified=false`; its same-Runner
[final PNG](/private/tmp/auv-osworld-v1-positive-settle.YmIGAE/output/chrome-positive-settle-l/final-screenshot.png)
has SHA256 `51c3ef4b56390af6327cc0e00a9467612eedf86e4ce57a21605db94cd5210b3a`.
After the two-second wait, the `New folder` dialog still displayed
`Favorites` and Save; the pinned evaluator still returned `0.0`. A final
delay alone is therefore insufficient. The missing evidence is the UI state
*before* the Save click; do not infer whether the click raced dialog readiness
or otherwise failed from these two terminal images. No coordinates or action
order were changed within either episode.

### Attended same-Runner Chrome V1 positive gate

An independently fresh V1 overlay on `liet-gpu-1` tested the benchmark-local
interactive entry from head `bf5c6c04` (host Mach-O binary SHA256
`441acdbb3c06b9f916f5b090eb4c94c257643d2105d2a36cdfe758af847b4bf8`).
The [operator config](/private/tmp/auv-osworld-v1-interactive-1006m/config.json)
SHA256 was `c471ca2737bbe5dc753f45e869d40aef4ab9d9b167165cd417dc434783614afc`;
the task remained the pinned Chrome V1 JSON/revision. Boot validated a fresh
qcow2 backing-file overlay and Pod UID. Install paired Device ID
`999ebf70f12d169950e11cc4640d242c38a6f528e6b23ca985618fd79842cd7e`;
the pinned upstream setup method returned true. An initial launch without the
phase-owned port-forward failed at connection before creating a Run or
delivering GUI input. The attended launch then used the adapter's UID-checked
`forward(auv=True)` context, not the OSWorld GUI relay.

One AUV Runner/Run, `a9db99a6a866414d75b5912392c43d02`, captured the
initial Chrome desktop and interleaved the same five typed actions used by
the static attempts: Ctrl+Shift+B, right-click `(400,128)`, click Add folder
`(500,532)`, type `Favorites`, and click Save `(1269,638)`. The
[request record](/private/tmp/auv-osworld-v1-interactive-1006m/chrome-interactive-m/action-requests.json)
SHA256 is `9ae2884ab3e594ad5ff9726e1c9fa7f6fdefda6d9a9f67886a9c9c647b20495c`.
All five original typed deliveries reported successful
`foreground_system_events` attempts with `verified=false`; delivery alone
did not prove the UI state. The [pre-Save AUV checkpoint](/private/tmp/auv-osworld-v1-interactive-1006m/chrome-interactive-m/checkpoint-0005.png)
SHA256 `5e108b7e251aa6fcfa1ee0c4fc05fb4aff11a5e655631fad727ac2d91bcdd695`
shows `Favorites` entered, `Bookmarks bar` selected, and Save at the scripted
coordinate. The [post-Save checkpoint](/private/tmp/auv-osworld-v1-interactive-1006m/chrome-interactive-m/checkpoint-0006.png)
SHA256 `716759054a53669dd35ae69fb64156c1985e82060ab7a66bd165bab8333bc1bf`
shows the dialog closed and `Favorites` on the bookmark bar. The
[checkpoint index](/private/tmp/auv-osworld-v1-interactive-1006m/chrome-interactive-m/checkpoints.json)
SHA256 is `6a71e0fb48147a03c6ab575bfe14ac47f31f4f9322968696d998df72419ed4db`;
the final same-Runner PNG SHA256 is
`bbcdf2dc5ff1db647bea5f4067e7ef6f06574f29c72dec684f597596455861b3`,
matching the [atomic sidecar](/private/tmp/auv-osworld-v1-interactive-1006m/chrome-interactive-m/action_evidence.json)
SHA256 `c7329e6576352edd1781b8105af4244ac85c720134e64c339353d59db70eb535`.

The unchanged pinned V1 `DesktopEnv.evaluate()` method-body bridge returned
raw score **`1.0`** after official postconfig. This is one attended,
task-directed positive gate, not an unattended batch, full upstream provider
run, or representative OSWorld success rate. Inter-action observation and
elapsed time differ from the failed static replays; the comparison does not
isolate which factor made Save effective. Direct phase invocations did not
create a six-phase `batch_runner.py` ledger. UID-safe reset removed only the
task-owned Pod/Service/proxy; an independent label query found none, and the
V1 hot PVC/PV UIDs stayed `34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`.

### Guest-local Unix-socket interactive and cancellation gate

A fresh official V1 qcow2 overlay on `liet-gpu-1` tested the no-pairing path on
2026-10-06. This was an infrastructure/action gate, not an OSWorld task or
evaluator score. The runtime image was pinned to
`happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`;
the V1 hot qcow2 SHA256 was
`6bf667a852b3c307f61d9f09c42559351f45e0607e428b4997becf534cf4d313`.
Ubuntu 22.04 amd64 binaries were built from source commit `49745a5a` (the
`auv` SHA256 was `327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e`
and `auv-osworld-action` SHA256 was
`6253f8f0a4b567532f27c17dc67f233617e1ff2813543e7b881644f9f113e4ea`).
The cancellation integration-test ELF was rebuilt from test-only fix
`980c1ec9` (SHA256
`53050bbae1643c7876eb3c8f2680d7297940a5a739bad289c4cba97af435c314`).
All three SHA256 values and Ubuntu/glibc linkage were checked inside the guest.

OSWorld's setup plane was used only for inert binary upload and temporary SSH
provisioning. `auv serve`, the action entry, and integration tests were launched
through pinned-host-key SSH inside the guest; all GUI input and captures used
the guest-local AUV owner Unix socket. No CUA, VNC input, xdotool, PyAutoGUI,
or OSWorld `/execute`/`/setup/execute`/`/setup/launch` GUI relay was used.
The guest AUV Device ID was `e18efaf99865095bf9b7f6781bb4ded819a70dccacc5c24ec09a8ad718424964`.

The ignored same-Runner tests `interactive_capture_before_and_after_typed_action_keeps_one_run`
and `interactive_eof_keeps_run_id_and_cancels_without_final_png` passed. The
first cancellation test attempt used 1001 plan actions against a 1000-action
limit, so it failed before creating a Run or holding input; the test-only fix
changed it to `MOUSE_DOWN` plus 999 moves. The corrected
`foreground_runner_cancellation_preserves_run_and_releases_input` passed
1/1 in 0.37 s (exit 0), including a second Run that reacquired input after
termination. This is a release/reacquisition assertion by the same test, not
an independent X11 event-receiver trace.

A separate persistent interactive process sent capture, typed `MOVE_TO`
`(300,300)`, capture, then finish through one Runner. Run
`72fba5ca8f75389c0f92269f22b87d72` completed (exit 0) with two checkpoint
PNGs and a final PNG; the [checkpoint index](/private/tmp/auv-osworld-v1-guestlocal-1006g/evidence/interactive-1/checkpoints.json)
SHA256 was `a49475cb0629691e1ccab4a7fec7af3d5df0d38b758b957a28e49edbef73c2a8`.
The [final PNG](/private/tmp/auv-osworld-v1-guestlocal-1006g/evidence/interactive-1/final-screenshot.png)
SHA256 was `20615a2d38a06210225c3fdf94320f513054b2387a3b8f11acc81aa5f52a38de`;
the [atomic sidecar](/private/tmp/auv-osworld-v1-guestlocal-1006g/evidence/interactive-1/action_evidence.json)
SHA256 was `39289b8559d9476c59c8837c0874fda8cbaa20a2a6080af24366f20e2a2ee7c1`.
The move's `InputActionResult` reported successful foreground system-event
delivery with `verified=false`; an unchanged screenshot does not prove pointer
motion. A held `MOUSE_DOWN` followed by stdin EOF produced expected exit 1,
Run `5435a94d72a1d64deebcb6b78a1427f7`, terminal sidecar
`final_artifact=null`, and no final PNG. The [EOF sidecar](/private/tmp/auv-osworld-v1-guestlocal-1006g/evidence/eof-1/action_evidence.json)
SHA256 was `83bb87415d18aeede68d2afd060a9dacac86e3aa1dbfb9b946c7f90d5b8db11f`.

The task-owned guest Pod, proxy Pod, and Service were removed with UID
preconditions. The task-owned build PVC `auv-osworld-build-1006g` was deleted
with UID precondition, and its `Delete`-reclaim PV disappeared; temporary SSH
credentials and local forwards were removed. The retained V1 hot PVC/PV stayed
Bound with UIDs `34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`. The local
[cleanup journal](/private/tmp/auv-osworld-v1-guestlocal-1006g/cleanup-journal.json)
records exact task-owned UIDs.

### Scripted Chrome V1 visual-controller batch

On 2026-10-06, a fresh paired-remote V1 overlay on `liet-gpu-1` ran the
fixed Chrome bookmark-folder task `2ad9387a-65d8-4e33-ad5b-7580065a27ca`
through the six-phase batch runner. The predeclared denominator was **one**.
The task JSON matched SHA256
`4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c`;
the pinned V1 revision was `b138d348256078fa634fc3b73567a7337c793e6b`.
The [batch input](/private/tmp/auv-osworld-v1-scripted-1006n/batch.json),
[episode config](/private/tmp/auv-osworld-v1-scripted-1006n/episode.json), and
[generated manifest](/private/tmp/auv-osworld-v1-scripted-1006n/manifest.json)
record the selected node, runtime image, qcow2, binaries, ports, and resource
names. The interactive action binary SHA256 was
`441acdbb3c06b9f916f5b090eb4c94c257643d2105d2a36cdfe758af847b4bf8`;
the five-action policy SHA256 was
`4e7c4fde21e452b2007ced6f25e2649e40b1c37094bee2b064da6c9ba73a28dc`.
The controller used pinned Tesseract 5.5.2 and English model bytes to gate
the next AUV action on spatial OCR of AUV checkpoint PNGs. It needed seven
observations for the four gates; all gates passed. No CUA, VNC input,
xdotool, PyAutoGUI, or OSWorld GUI relay was used for the task actions.

The [durable ledger](/private/tmp/auv-osworld-v1-scripted-1006n/output/ledger.json)
reports all six phases `ok`, no failure layers, and the pinned method-body
evaluator's raw score **`1.0`**. The single AUV Run ID was
`663a9979ded03bd661e187da564b578f`; the
[controller decisions](/private/tmp/auv-osworld-v1-scripted-1006n/output/chrome-scripted-n/controller_decisions.json)
SHA256 was `2ea4b1be695eaf89fe7a5a461665d94c67071dc28fa56c3f3066cbdeff3788ea`.
The [final AUV PNG](/private/tmp/auv-osworld-v1-scripted-1006n/output/chrome-scripted-n/final-screenshot.png)
SHA256 was `9709691aafa74395e70854daa887e14104fbaeca7f48812db7a90a9b21179955`;
the [atomic sidecar](/private/tmp/auv-osworld-v1-scripted-1006n/output/chrome-scripted-n/action_evidence.json)
SHA256 was `c81c47f05d675e1889ae41bb9d205c384ec8860a8fb4b657f17b95834641bb93`.
Reset removed only its recorded proxy Pod, Service, and runtime Pod with UID
preconditions. The retained hot PVC/PV stayed Bound with UIDs
`34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`.

This is one deterministic scripted baseline, not an autonomous-agent score
or an official full-provider benchmark result. Its OCR gates were checked
against archived positive and failed-static screenshots before this run;
their robustness across other Chrome layouts is unproven. A successful
foreground action still reports input delivery, not independent X11 receiver
verification. At the time of this Chrome-only run, scripted VLC task solving
remained open; its later result is recorded below. V2.1 batch evaluation
remains open.

### Attended VLC V1 task-directed gate

A separate fresh V1 overlay on `liet-gpu-1` completed the pinned VLC task
`5ac2891a-eacd-4954-b339-98abba077adb` on 2026-10-06. This was an
**attended** paired-AUV episode, not the scripted Chrome batch or a frozen
two-task denominator. The selected task JSON SHA256 was
`4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da`.
The OSWorld setup plane prepared the guest; only AUV delivered task GUI
input and screenshots. No CUA, VNC input, xdotool, PyAutoGUI, or OSWorld GUI
relay was used for the action phase.

The operator used one interactive AUV Run
`fbdf8d9d91d6eb6fa9a195f5aa00aaae`: capture VLC; Ctrl+P; capture Simple
Preferences; click `All` at `(638,898)`; capture a fully redrawn Advanced
Preferences page; focus search at `(696,253)`; type `play and exit`; capture
the filtered Playlist tree; click Playlist at `(642,310)`; capture the
`Play and exit` checkbox at about `(906,395)` **already unchecked**; click
Save at `(1211,901)` without toggling the checkbox; capture the returned
main window; finish. The UI state matters: setup had written
`play-and-exit=1` to disk after launching VLC, while its in-memory
preferences displayed the default unchecked value. Save rewrote the disk
configuration. Do not turn this into a blind toggle action.

The [checkpoint index](/private/tmp/auv-osworld-vlc-attended-1006a/vlc-attended-a/checkpoints.json)
SHA256 was `cc762a48a00c5483ca9b265f10e3415b2a50de1be7dbd09a4e94c8aac2c060fa`.
The [target-checkbox checkpoint](/private/tmp/auv-osworld-vlc-attended-1006a/vlc-attended-a/checkpoint-0005.png)
SHA256 was `15199be39279b027f3d602925b558e93d585f908d0188e7a478981144331faf5`;
the [action request index](/private/tmp/auv-osworld-vlc-attended-1006a/vlc-attended-a/action-requests.json)
SHA256 was `d68a3097e8152a3360e13c3b79d198bc47dc6ff42c3556e72370956f4a135a77`.
The [final PNG](/private/tmp/auv-osworld-vlc-attended-1006a/vlc-attended-a/final-screenshot.png)
SHA256 was `64c0561f2a70989ed33caeeffbbfb0223068d95390f46ed1bfe857e7351ac9ac`;
the [atomic sidecar](/private/tmp/auv-osworld-vlc-attended-1006a/vlc-attended-a/action_evidence.json)
SHA256 was `e195a8cf03547d45a7a3d6a8980b114217fdeef9e31319f6163c8cec02067462`.
The pinned evaluator's transcribed [result](/private/tmp/auv-osworld-vlc-attended-1006a/gate-result.json)
(SHA256 `0fd331dd831589be00d1f361cfa8b3c5adc0111d24c5ef71971d830f60f1c1df`)
returned raw `1.0`. All typed input attempts reported delivery succeeded
with `verified=false`; semantic success rests on the evaluator, not that
delivery flag.

UID-preconditioned reset removed the task-owned runtime Pod
`c0d4dcc1-689e-4b7c-9961-bd9d776a8d04`, Service
`62099692-8c82-42f6-944a-aa271db79134`, and proxy Pod
`650b3c53-02d2-46ee-9c53-959d292fe2f8`; a subsequent label query was
empty. Local forwards and action process stopped. The hot PVC/PV remained
Bound with UIDs `34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`. One fresh screenshot is not
enough to freeze a robust visual policy: full-frame OCR missed the advanced
title in one capture, and OCR alone cannot prove checkbox checkedness. A
replay with an explicitly checked/unchecked image predicate is still needed.

A second fresh VLC V1 attempt to capture that same checkbox in ON/OFF
states did **not** reach the target. In paired AUV Run
`b2c59d714831d56c157f60c3c8ba3cef`, Ctrl+P first required another
capture before Simple Preferences appeared. After AUV clicked `All`, the
title changed to Advanced but the right pane continued showing stale Simple
content across three checkpoints, including one after more than seven
seconds. The controller/operator sent `abort`; no target click, Save, or
evaluator followed, so this attempt has **no score**, not `0.0`. The
[checkpoint index](/private/tmp/auv-osworld-vlc-onoff-1006b/vlc-onoff-b/checkpoints.json)
SHA256 was `945b92d3ef73f4e192076cec70f9e4165345a36c62cd649d8b6010350aa9f876`;
the [action request index](/private/tmp/auv-osworld-vlc-onoff-1006b/vlc-onoff-b/action-requests.json)
SHA256 was `5d04013a59c41ee6134a62ae9ff9f6cb5a4b6ba5944eb735092ceb3da5f14b74`.
The aborted sidecar had `final_artifact=null` and SHA256
`af10518f243d411a45178853e5438ed46b15f1a34274e9b4aa6b22e721dac427`.
UID-safe reset removed the task-owned proxy Pod
`9c3328fd-5b9b-4b50-a287-a78343d7bff0`, Service
`bbf40601-2437-4527-b648-b1b59e0016fe`, and runtime Pod
`5b2663ec-0b3d-40b0-b17a-76504274106d`; no matching resource or local
forward remained. Hot PVC/PV UIDs were unchanged. This reproduces the
intermittent stale-pane boundary and leaves the checked-target pixel
control uncollected; do not freeze an unattended VLC policy from the single
positive episode.

A bounded live repro then exercised the exact redraw path repeatedly in
one more fresh V1 guest without saving or evaluating. The task-owned
[diagnostic script](/private/tmp/auv-osworld-vlc-loop-1006c/live_repro.py)
(SHA256 `46e07877eab32233c6970b2ce4c9c41e122b1dd78f2b1035858b234b84e4aecf`)
used only paired AUV actions/captures and pinned Tesseract 5.5.2. Each
cycle opened Preferences with Ctrl+P, verified Simple content, clicked All,
classified both title and right pane, pressed Escape, and verified the
return to VLC main. In one persistent Run
`aeec82015d4aad6628b09e6bcbb3b51e`, cycles 1 and 2 were `RED_STALE`
(same PNG SHA256
`f42125b07bda2b962daff7a0a47fd3239ea73a065e203d2a4fed1706affd5585`)
and cycle 3 was `GREEN_ADVANCED` (PNG SHA256
`0644caa212430957fd104ffb5c835dd44d993bac0f32f4713e3fc8d0dcf5dc4c`).
Cycle times were 11.633, 12.444, and 12.776 seconds. The
[per-cycle signal record](/private/tmp/auv-osworld-vlc-loop-1006c/loop-results.json)
SHA256 was `d847a94025b0eb08449909ea14f0c041cf15a976be22a022a39d5d6e8cbe6926`;
the [checkpoint index](/private/tmp/auv-osworld-vlc-loop-1006c/vlc-loop-c/checkpoints.json)
SHA256 was `a4ba961c3599c6020061765f475cd5e60b6b52d6f0e53f807d5e58c8ff66dc6c`.
The Run was intentionally aborted with no final artifact and **no
evaluator score**. UID-safe reset removed runtime Pod
`15bfa184-a2b2-475c-a939-a89b85f7186d`, Service
`2478ad49-8b15-4d1c-b42b-a80e5ce05bc2`, and proxy Pod
`985c94f8-9c93-4a79-84e4-ddd88d9afcb3`; the hot PVC/PV UIDs remained
unchanged. This gives a reproducible red/green symptom signal within one
guest, not a root-cause diagnosis or a validated recovery policy.

A later fresh attempt also stopped at an ambiguous transition frame. The
[gate record](/private/tmp/auv-osworld-vlc-toggle-1006d/gate-results.json)
(SHA256 `5a23f4db62caae26f57453774afef2ecb5a05814afe1a4beba3a75e16f01f6fb`)
records AUV Run `9452e6648ac2461fd315a40b02777c6a`: the window title
said Advanced after clicking All, but the right-pane signals did not yet
establish the advanced page. It aborted without targeting the checkbox,
saving, or evaluating. This attempt therefore has no task score. The
transition PNG SHA256 was
`4032054ccd6be8b2b87d114af4539e9ceabf6bc0a1a31befc1975427cffe47e8`.

The next fresh guest produced a complete **attended** same-control state
sequence. In AUV Run `73b98f4195e825b1bf1e45698501e8cd`, the first
Ctrl+P / All attempt showed both the Advanced title and new right pane.
After searching `play and exit` and selecting Playlist, the target checkbox
was observed unchecked, then clicked to checked, then clicked back to
unchecked. All three AUV checkpoints retained the Advanced/Playlist/Play
and exit spatial OCR signals. In the fixed 11×11 target region
`x=901..911, y=390..400`, the count of pixels darker than 100 changed
`0 → 20 → 0`; same-frame checked and unchecked controls counted 20 and 0.
The [unchecked](/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0005.png),
[checked](/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0006.png),
and [unchecked again](/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0007.png)
PNGs have SHA256 values
`ed353ea25eb640af81af251e9cf8ce9cc57310f0e6faf8db1de6d032db758f7b`,
`102acabb798d281d9c6dc36bc998c27d4d049224b83672946db85726c20ede7c`,
and `0c27c2e8c3507ad06991b9ab612af8a605f78237a694983e44e9c614bd35a025`.
After Save, AUV observed the VLC main window. The pinned V1 evaluator
returned raw `1.0` for task
`5ac2891a-eacd-4954-b339-98abba077adb` (task JSON SHA256
`4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da`,
upstream revision `b138d348256078fa634fc3b73567a7337c793e6b`). The
[gate record](/private/tmp/auv-osworld-vlc-toggle-1006e/gate-results.json)
SHA256 was `6c4b9a7c7581b05e1877d7233951c7a6366d8307f0dcc800a4aaec116cefd0c1`.
UID-safe reset removed only this attempt's runtime Pod
`5ce206f8-ddcc-4166-b1b4-2e415e00634e`, proxy Pod
`38f0f1e3-9fe4-4b03-b6b4-daebae7e1d01`, and Service
`dc59e675-ae9d-4d9e-ac21-5d98da03b1ec`; the hot PVC/PV UIDs stayed
unchanged. A pre-boot local journal initialization error was recovered by
adopting only the observed task-owned Pod after verifying its UID, labels,
node, image, and read-only image mount. This episode validates the image
predicate within one attended Run; a fresh scripted-controller replay is
still required before calling it an automated baseline.

The first fresh **scripted** VLC six-phase replay did not solve the task.
The predeclared [manifest](/private/tmp/auv-osworld-vlc-scripted-1006f.KHtQ2v/manifest.json)
(SHA256 `c601f1f8fd839ce1cd964f9ae2811b6ab71f3f9d77d2dea8bc57f0aef89b7dfa`)
selected one pinned V1 VLC episode. Boot, install, and setup passed on a new
read-only-base qcow2 overlay. AUV Run `8e4d8c3aba38a256e534546c0e7a8dcc`
reached MAIN, SIMPLE, GREEN_ADVANCED, verified search, and GREEN_TARGET
in sequence. The initial target was unchecked with same-frame checked and
unchecked controls at 20 and 0 dark pixels. After the fixed AUV click at
`(906,395)` and a one-second wait, the next AUV image still measured the
target at 0 dark pixels; the visible checkbox appeared gray. The
[before](/private/tmp/auv-osworld-vlc-scripted-1006f.KHtQ2v/output/vlc-scripted-f/checkpoint-0005.png)
and [after](/private/tmp/auv-osworld-vlc-scripted-1006f.KHtQ2v/output/vlc-scripted-f/checkpoint-0006.png)
PNGs have SHA256
`7be375f22be9b689250074a118df6c337f09b6dd3faa316fa48d8bee3d812296`
and `43f0fe3551cf2acb6cdf51f19367db3c2b02bf4600959db54fbbea0c2c079c5b`.
This observation does not identify whether input delivery, UI focus, or
redraw timing caused the unchanged state. The fail-closed controller did
not click Save; its [decision trace](/private/tmp/auv-osworld-vlc-scripted-1006f.KHtQ2v/output/vlc-scripted-f/controller_decisions.json)
SHA256 was `f91fb9fb83dfd15815c5adddd1170f83270791c93fff64ca10c079e509dbb8a5`.
The exact pinned evaluator returned raw `0.0`; the
[ledger](/private/tmp/auv-osworld-vlc-scripted-1006f.KHtQ2v/output/ledger.json)
(SHA256 `fc14b7d834ab53c8ab3ba94624d9208ca058f74db5784261a2496993aee625f9`)
records `exit_failed` at action, evaluator completion, and successful
UID-preconditioned reset. Reset removed only this episode's proxy Pod
`a824199f-5cf4-45d8-89aa-d58dcfdf981b`, Service
`13254ca9-830d-4902-a453-8b7a31439f79`, and runtime Pod
`a2d60127-b826-4d4d-9759-bd01ba7163cd`. The V1 hot PVC/PV UIDs
remained `34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`. This is a scored failed
scripted attempt, not evidence that the checkbox is generally inaccessible:
the attended 1006e sequence above did reach both states.

A revised fixed policy removed the unnecessary ON/OFF exercise from the
task-solving path. It still requires MAIN, SIMPLE, a fully rendered Advanced
right pane, the search query and Playlist target, and an initially unchecked
checkbox calibrated against checked/unchecked controls in the same AUV
frame. Only then does it click Save. The next fresh six-phase
[manifest](/private/tmp/auv-osworld-vlc-scripted-1006g.oP66D0/manifest.json)
(SHA256 `f3631b0f3931ba466c82294daf4618ea2499aa7c9042a693549bb780633f77da`)
predeclared one pinned VLC task and policy SHA256
`94c64900027c6ac4488178d578a824eb24e864dbce913662e76a1bfc80cc09cb`.
In AUV Run `a36de3adabdf8132819ca23b7fcc5e09`, the first Advanced
transition remained `AMBIGUOUS` through four captures. The controller
returned to MAIN and reopened Preferences; the second opening reached
`GREEN_ADVANCED`. It then observed target `unchecked` with pixel counts
0/20/0 (target/checked control/unchecked control), clicked Save, and
observed MAIN. The [decision trace](/private/tmp/auv-osworld-vlc-scripted-1006g.oP66D0/output/vlc-scripted-g/controller_decisions.json)
SHA256 was `c9894b2b38647ec562c532c61fa8122e17caced93dbdc1090dc695e9adceb4f6`;
the [AUV sidecar](/private/tmp/auv-osworld-vlc-scripted-1006g.oP66D0/output/vlc-scripted-g/action_evidence.json)
SHA256 was `22a86d417229749c0668f7bd0bdbda829bce66752ac436faaa39945428160b6e`.
The final AUV PNG SHA256 was
`6f2bb3bd4763943f5c3ffaceb0ac5c0d5a9c45e736e1d5a3f2dc62cb08398ae2`.
The exact pinned V1 evaluator returned raw `1.0`; the
[ledger](/private/tmp/auv-osworld-vlc-scripted-1006g.oP66D0/output/ledger.json)
(SHA256 `e1f39bbf0bdf6d29a5c05c6410e07f043b6fafe739722eda39ceae076825ebea`)
records all six phases completed, no failure layers, denominator one, and
verified AUV evidence. UID-safe reset removed only proxy Pod
`79864834-1f95-4dcb-9410-9196817c0508`, Service
`e1ecf678-9cb1-4e8c-a2d2-ae1abc14de50`, and runtime Pod
`0f30dbf6-7fdb-494c-a6a2-a502fe694840`; the retained V1 hot PVC/PV
UIDs were unchanged. This is a deterministic scripted VLC baseline,
not a general autonomous-agent score or benchmark-wide rate.

### Predeclared two-task scripted V1 batch

The next [fixed manifest](/private/tmp/auv-osworld-v1-two-scripted-1006h.OaGEBO/manifest.json)
(file SHA256 `b5b20c55a972dd6831ab7ccd844a2d3532bbd9b2d5d92053a1c3035fc593cba7`)
predeclared Chrome `2ad9387a-65d8-4e33-ad5b-7580065a27ca` and VLC
`5ac2891a-eacd-4954-b339-98abba077adb` before starting either VM;
denominator **2**. The two controllers shared the measured host action
binary SHA256 `ee84942d59239f274b2b35d6a56707e621e29fff3680552d29ebaa2082f30bb0`
but had separate fixed policy hashes. The runner executed them serially on
distinct disposable V1 qcow2 overlays with separate task-owned Kubernetes
names and local ports. All task GUI observation/input went through AUV;
setup/evaluator remained confined to their audited non-GUI method bodies.

The [final ledger](/private/tmp/auv-osworld-v1-two-scripted-1006h.OaGEBO/output/ledger.json)
SHA256 was `2376326fddf0944bd82405d010a2ed0b9a0cf51fa0a767c241a1d98512304f07`.
Both episodes completed all six phases with no failure layers and raw pinned
evaluator scores `1.0` (Chrome) and `1.0` (VLC). The arithmetic mean is
`1.0` over this **selected scripted denominator of two** only. Chrome AUV
Run `4f07fbbd24148cbfc219fb70d15d6647` needed one observation retry
at each of its four spatial gates; its
[controller trace](/private/tmp/auv-osworld-v1-two-scripted-1006h.OaGEBO/output/chrome-scripted-h/controller_decisions.json)
SHA256 was `7f81766c5e5b485e3a44d1e1685c7ed4d15b2131db3327fc8b85889a9036974e`,
and its final AUV PNG SHA256 was
`f4c4bda0d9e1fd791140f5233ceb2dad383d70ee62b296e25407278072798dbb`.
VLC AUV Run `dbe6323a900916c27130092fe61566f0` reached the target
unchecked with same-frame pixel controls 0/20/0, saved, and returned to
MAIN; its [controller trace](/private/tmp/auv-osworld-v1-two-scripted-1006h.OaGEBO/output/vlc-scripted-h/controller_decisions.json)
SHA256 was `08f82dfde40821c04411ebbd6001802a80fb1f82b52ee68dbb3e1ca24a708267`,
and its final AUV PNG SHA256 was
`01b326813ca392f37a02ccfea0cce8523b19353e50f5f6590b71f0d7ad75070f`.
The runner verified each controller trace, Run sidecar, and final PNG hash.
Both UID-safe resets completed; a batch-label query returned no Pods or
Services. The retained V1 hot PVC/PV UIDs were still
`34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`. This is infrastructure plus
deterministic task-specific scripts, **not** an autonomous-agent benchmark
rate or full upstream provider run.

### Two exploratory blinded Codex sub-agent V1 canaries

On 2026-10-06, the operator ran two **separate, sequential, selected** V1
canaries on fresh `liet-gpu-1` QEMU overlays. Each had a pre-written
single-task [Chrome protocol](/private/tmp/auv-osworld-blind-codex-chrome-1006.eAc2gG/protocol.json)
(SHA256 `987ba41a9af403961e8b08bd1e0c5629c762f98991b8bfebae6dd7b273190648`)
or [VLC protocol](/private/tmp/auv-osworld-blind-codex-vlc-1006.ojj3ob/protocol.json)
(SHA256 `889ffeab695e8920e6757d8d4cb7ce8ef9acf8bef12a0c13f32c855d318aad53`),
not a predeclared two-task batch. The operator booted, installed and paired
guest AUV, and ran official task setup before spawning a new Codex sub-agent
with `fork_turns=none` for each task. The sub-agent was given only the official
natural-language instruction, the paired AUV CLI endpoint, and limits of
600 seconds, 32 AUV actions, and 32 AUV screenshots. It was instructed to
avoid task/evaluator files and every non-AUV GUI path. The operator alone ran
the pinned evaluator after the sub-agent stopped. This is prompt-level
blindness, not a separately enforced tool sandbox or pinned-model inference
service; the Run store proves AUV operations, not the absence of all other
possible reads.

| Selected task | Agent AUV evidence | Pinned raw score | Interpretation |
| --- | --- | ---: | --- |
| Chrome `2ad9387a-65d8-4e33-ad5b-7580065a27ca` | 5 `auv.driver.input_action_result` artifacts and 5 `auv.driver.display_capture` artifacts; [final PNG](/private/tmp/auv-osworld-blind-codex-chrome-1006.eAc2gG/chrome-blind-1006/agent-runs/artifacts/fd62abb7-a717-03ff-5ff8-293b3a06ad84/01a110cc-707f-71dd-9edd-81b4c9b24113.png), SHA256 `8f898311c478ead4e9174661289214752f86355c058c5711be1228919511edb3` | `1.0` | The final AUV screenshot visibly shows `Favorites` on Chrome's bookmarks bar. |
| VLC `5ac2891a-eacd-4954-b339-98abba077adb` | 12 input-result artifacts and 12 capture artifacts; [final PNG](/private/tmp/auv-osworld-blind-codex-vlc-1006.ojj3ob/vlc-blind-1006/agent-runs/artifacts/3b7ee525-24ee-3f1a-3ed6-27c968e872c1/01a110d7-bb95-73fc-a9ee-b430617d0b05.png), SHA256 `a32faa8772564aebc7f48615acdbe471ae2ab971d66e4ac32e895bc6c3ed86ea` | `1.0` | The agent reported that `Play and exit` already appeared off, enabled `Pause on the last frame of a video`, saved, and reopened Preferences. The evaluator verifies the final `play-and-exit=0` state, not the agent's causal diagnosis. Saving another option may have rewritten that setting; do not claim it identified the intended control. |

The Chrome Run [records](/private/tmp/auv-osworld-blind-codex-chrome-1006.eAc2gG/chrome-blind-1006/agent-runs/records.jsonl)
and VLC Run [records](/private/tmp/auv-osworld-blind-codex-vlc-1006.ojj3ob/vlc-blind-1006/agent-runs/records.jsonl)
contain one completed Run per CLI invocation, not one shared agent Run.
Both evaluator calls used V1 upstream revision
`b138d348256078fa634fc3b73567a7337c793e6b` and the audited selected
task hashes. Both UID-preconditioned resets removed only the respective
task-owned runtime Pod, Service, and proxy Pod; no matching batch-labelled
resources remain. The hot PVC UID
`34143535-4ac2-42f2-a443-08db3f6b49ff` and PV UID
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73` were unchanged. These are two
successful **selected, exploratory Codex-agent canaries**, not an official
OSWorld completion rate, reproducible model benchmark, proof of general VLC
reasoning, or evidence about guest-local/V2.1 topology.

### Live agent gateway/transport negative control

At head `d20a2134`, a fresh Chrome V1 overlay on `liet-gpu-1` exercised
`ForegroundActionTransport` and `AgentActionGateway` together, without a
model. The task-owned [config](/private/tmp/auv-osworld-gateway-live-1006.6L9TUQ/config.json)
SHA256 was `7d8ce85ae384b99572241864649ffaca5007991992ad4b3a9fd39911955388d1`;
the temporary fixed [action harness](/private/tmp/auv-osworld-gateway-live-1006.6L9TUQ/live_gateway_control.py)
SHA256 was `7cffb3ba8812ff8a59b5b7b6de3008f1221fc62f1d4410cfa3e7324394d47271`.
The pinned base qcow2, live QEMU/KVM overlay, guest AUV install/pair, and
official Chrome setup passed their existing identity checks. No OSWorld API,
VNC, or non-AUV tool delivered task GUI input.

One AUV Run `ec11dfc24f30b56594a906e2d1e269c1` completed four ordered
requests: `capture → MOVE_TO(600,500) → capture → finish`. The action cited
the first AUV checkpoint's exact Run ID and SHA256
`9483f01dc196f6270b9303c56e7c89043294c112ed11c9df4cbc59020d952dfd`.
The [decision trace](/private/tmp/auv-osworld-gateway-live-1006.6L9TUQ/chrome-gw-live-1006a/agent_decisions.json)
SHA256 was `9c106572b2bc0f504d97ddfd3e278167fb7e57f260b132bd77780d6111ee6d46`:
four matching receipts, one action, two captures, `status=finished`, and no
pending forward. It matched Rust `checkpoints.json`,
`input-action-results.json` (SHA256
`1644250b7b98ef77a7ccc4b6440809cbf006fda817fafc96f2c7af88d8ffdea1`),
and `action_evidence.json` (SHA256
`80b36f1c9741b583938699657b41abb188c6f2451bfea3a61dc3408924cc712e`).
The second and final AUV PNGs both had SHA256
`4acba88072bfb3e384dab2d67aba958137b511812afb32b2961b64721ac813c7`.
The original `InputActionResult` selected `foreground_system_events`, with
`succeeded=true` and `verified=false`; no semantic effect was inferred from
delivery alone. The pinned Chrome evaluator returned raw `0.0`, as expected
from this non-solving pointer move.

UID-preconditioned reset removed only proxy Pod
`65039cdc-c4ab-4687-af8b-1d1750974c1f`, Service
`dbbfa2ef-c39b-42c4-979e-be9e1b14abfc`, and QEMU Pod
`d2da7a24-8035-45a2-a6a7-3d4cefbd663f`. An independent query found no
matching Pods or Services. The hot V1 PVC/PV UIDs remained
`34143535-4ac2-42f2-a443-08db3f6b49ff` and
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73`, both Bound. This closes one
paired-remote **live gateway input/evidence path**, not a model/tool-sandbox
test, agent task pass, guest-local gateway run, or benchmark rate.

## Operational checklist

1. Select `liet-gpu-1` for the official QEMU/KubeVirt lane; reserve
   `neko-gpu-1` for the fast Xorg/HAMi fixture.
2. Pin both qcow2 SHA256 and runtime-image digest.
3. Import/extract on a PVC and verify bytes before boot.
4. Start QEMU with `/dev/kvm`, `NET_ADMIN`, explicit CPU/memory requests, and no
   GPU claim for the initial lane.
5. Keep port 5000 and the AUV pairing endpoint cluster-private or behind a
   short-lived port-forward. The OSWorld setup API can upload files and execute
   guest commands and must not be exposed through an unauthenticated Ingress.
6. Verify `/screenshot` and the OSWorld setup/evaluator APIs, then install the
   matching AUV binary in the guest as an environment setup step.
7. For paired mode, run `auv serve` in the guest and enroll its HTTP endpoint.
8. For non-paired mode, run the harness client in the same guest and share the
   guest Unix and X11 sockets; a host-side Pod socket is not equivalent.
9. Run a small task subset before provisioning mocked websites and GitLab.
10. Add KubeVirt/CDI only if snapshot/reset throughput or VM lifecycle is the
   measured bottleneck.

## Cluster state after validation

The disposable V1/V2 QEMU runtime Pods, TCP proxy, and Services were deleted
after evidence capture. The old fast `auv-osworld-x11` fixture on `neko-gpu-1`
was later Evicted for ephemeral-storage pressure; do not reuse it as a live
receiver. Four image PVCs were retained for reviewer reruns:

| PVC | Class | Capacity | Purpose |
| --- | --- | --- | --- |
| `osworld-v1-image` | `tns-iscsi` | 64 GiB | V1 cold release archive |
| `osworld-v1-hot` | `local-path` | 32 GiB | V1 node-local qcow2 on `liet-gpu-1` |
| `osworld-v2-image` | `tns-iscsi` | 80 GiB | V2.1 cold release archive |
| `osworld-v2-hot` | `local-path` | 32 GiB | V2.1 node-local qcow2 on `liet-gpu-1` |

Deleting a `local-path` PVC deletes its only hot copy. The immutable cold
archive is the recovery source, so remove hot PVCs only when the rerun latency
is acceptable.

## Sources

- [OSWorld README and provider guidance](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/README.md)
- [OSWorld Docker provider implementation](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/desktop_env/providers/docker/provider.py)
- [OSWorld-V2 README](https://github.com/xlang-ai/OSWorld-V2/blob/acdd3493808e716825975b0f0208194bb2faf3c3/README.md)
- [OSWorld-V2.1 pinned release manifest](https://github.com/xlang-ai/OSWorld-V2/blob/acdd3493808e716825975b0f0208194bb2faf3c3/benchmark_releases/osworld-v2.1.json)
- [OSWorld-V2 Docker provider implementation](https://github.com/xlang-ai/OSWorld-V2/blob/acdd3493808e716825975b0f0208194bb2faf3c3/desktop_env/providers/docker/provider.py)
- [Container-native XFCE image build](https://github.com/xlang-ai/osworld_image/blob/0d7f5d52d285c7399eb0c0be328af3e64d402c5c/docs/usage.md#docker-xfce-images)
- [KubeVirt installation and hardware-virtualization check](https://kubevirt.io/user-guide/cluster_admin/installation/)
- [KubeVirt CDI import and upload](https://kubevirt.io/user-guide/storage/containerized_data_importer/)
- [KubeVirt virtual hardware](https://kubevirt.io/user-guide/compute/virtual_hardware/)
