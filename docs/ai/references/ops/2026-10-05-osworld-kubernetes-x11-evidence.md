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
axis. OSWorld also permits a click without coordinates at the current cursor
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
after evidence capture. The fast `auv-osworld-x11` fixture remains running on
`neko-gpu-1`. Four image PVCs were retained for reviewer reruns:

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
