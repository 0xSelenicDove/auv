# OSWorld on Kubernetes with the AUV X11 driver

Date: 2026-10-05. Classification: owner-approved live validation and deployment
design. Cluster: `k8s.ihome.cat`, kubeconfig
`/Users/neko/.kube/config.d/ihome.conf`.

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
| `MOVE_TO` | `input.moveMouse` | implemented; not exercised in this cluster record separately from drag |
| `CLICK`, `RIGHT_CLICK`, `DOUBLE_CLICK` | `input.clickPoint` button/count | click live-validated; all buttons/counts covered by driver tests |
| `DRAG_TO` | `input.drag` or logical-mouse Runner calls | sampled drag live-validated after the rounding fix |
| `SCROLL` | `input.scrollPoint` | live-validated |
| `TYPING` | `input.typeText` | live-validated |
| `PRESS`, `HOTKEY` | `input.keys` | live-validated for a modifier chord and literal keys |
| `MOUSE_DOWN`, `MOUSE_UP` | logical-mouse Runner lifecycle | implemented and Xvfb-validated; no one-shot invoke command because cross-call ownership needs a session |
| `KEY_DOWN`, `KEY_UP` | held-key Runner lifecycle | implemented and Xvfb-validated; the public invoke surface exposes bounded `holdKeys`, not arbitrary cross-process ownership |
| `WAIT`, `DONE`, `FAIL` | harness lifecycle | no driver operation required |
| V2 `EXECUTE` | OSWorld setup/evaluator control plane | deliberately not a GUI action; do not expose arbitrary shell execution as an AUV input command |

An adapter is still required because OSWorld's `DRAG_TO` starts at the current
cursor while `input.drag` names both endpoints, and down/up actions require a
stable logical input owner across calls. The adapter should be a typed harness
consumer of the existing Runner APIs, not a parser that executes arbitrary
PyAutoGUI source.

Two observation differences remain explicit:

- xcap's X11 screenshot does not currently composite a cursor; OSWorld's guest
  screenshot path does.
- X11 window discovery/capture and clipboard paste are unsupported. Visual
  OSWorld agents can use display capture and foreground typing, but a task that
  specifically depends on clipboard semantics needs a separate approved slice.

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
