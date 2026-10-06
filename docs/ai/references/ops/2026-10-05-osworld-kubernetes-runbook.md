# Run OSWorld on Kubernetes with AUV

Date: 2026-10-05. This runbook turns the validated `k8s.ihome.cat`
experiment into a repeatable operator flow. It covers OSWorld V1 and the
recommended OSWorld-V2.1 release, QEMU/KVM startup, local access, AUV
installation, the two AUV connection topologies, reset, and cleanup.

The companion [evidence record](2026-10-05-osworld-kubernetes-x11-evidence.md)
contains the observed Runs, image hashes, evaluator results, and design
trade-offs. This document is an operating procedure; it does not claim that an
unattended full-suite AUV harness exists.

## Benchmark sizes

| Benchmark | Pinned counting source | Tasks |
| --- | --- | ---: |
| OSWorld V1 | upstream `evaluation_examples/test_all.json` and setup guide | 369 |
| OSWorld-V2.1 | `benchmark_releases/osworld-v2.1.json` `task_count` | 108 |

These totals are not directly comparable measures of difficulty. V2 contains
fewer, longer-horizon tasks and requires release-matched task classes, gated
assets, mocked websites, and provider images. The V1 repository changes over
time, so record the V1 Git commit used by every run. For V2, do not mix the
`osworld-v2.1` code, tasks, assets, website, or VM image with another release.

## Scope and safety boundary

This procedure uses a privileged Pod because the tested cluster has no KVM
device plugin and a plain `/dev/kvm` hostPath did not grant device-cgroup
access. The Pod also receives `NET_ADMIN`, and the guest control API on port
5000 can upload files and execute commands. Keep the namespace trusted and use
only short-lived local port-forwards. Do not expose ports 5000 or 8080 through
an unauthenticated Ingress or public LoadBalancer.

The validated host is `liet-gpu-1`. `neko-gpu-1` does not have `/dev/kvm` and
is only used for the fast container-native Xorg fixture. No GPU claim is needed
for the official OSWorld VM.

## Current ihome inventory

The namespace is `auv-x11-hami-test`. The retained volumes are:

| PVC | Storage | Contents |
| --- | --- | --- |
| `osworld-v1-image` | `tns-iscsi` | V1 cold release archive |
| `osworld-v1-hot` | node-local on `liet-gpu-1` | extracted V1 `System.qcow2` |
| `osworld-v2-image` | `tns-iscsi` | V2.1 cold release archive |
| `osworld-v2-hot` | node-local on `liet-gpu-1` | extracted V2.1 `System.qcow2` |
| `auv-osworld-workspace` | `tns-iscsi` | AUV checkout and validation builds |

The retained **older** guest-compatible X11-only AUV binary lives at
`/workspace/target-ubuntu2204-v2/release/auv` in the workspace volume. Its
validated SHA256 is
`7bc1f4256fa903d1bb660f73901d34c8aa2c0c85ed948473b9f8809b049cdc26`.
It is AUV 0.0.27 with no proven source commit, **not** the draft PR's current
head. Use it only to reproduce the older infrastructure control. The regular
Debian 13 artifact requires a newer glibc than Ubuntu 22.04.

## 1. Set the local context

The local machine needs `kubectl`, `curl`, `jq`, and `envsubst`. On macOS,
`envsubst` is provided by `brew install gettext`. Every new shell must select
the kubeconfig explicitly:

```bash
export KUBECONFIG=/Users/neko/.kube/config.d/ihome.conf
export OSWORLD_NAMESPACE=auv-x11-hami-test
kubectl config current-context
kubectl get node liet-gpu-1
kubectl -n "$OSWORLD_NAMESPACE" get pvc osworld-v1-hot osworld-v2-hot
```

Confirm that both hot PVCs are `Bound`. Confirm their node affinity before
booting:

```bash
for pvc in osworld-v1-hot osworld-v2-hot; do pv=$(kubectl -n "$OSWORLD_NAMESPACE" get pvc "$pvc" -o jsonpath='{.spec.volumeName}'); kubectl get pv "$pv" -o jsonpath="$pvc{': '}{.spec.nodeAffinity.required.nodeSelectorTerms[0].matchExpressions[0].values[0]}{'\n'}"; done
```

Both lines should name `liet-gpu-1`.

## 2. Select V1 or V2.1

Start with V2.1 unless the purpose of the run is comparison with V1:

```bash
export OSWORLD_VERSION=v2
```

Set the runtime variables:

```bash
case "$OSWORLD_VERSION" in v1) export OSWORLD_POD=osworld-v1-runtime OSWORLD_PVC=osworld-v1-hot OSWORLD_QCOW=System.qcow2 ;; v2) export OSWORLD_POD=osworld-v2-runtime OSWORLD_PVC=osworld-v2-hot OSWORLD_QCOW=System.qcow2 ;; *) echo "OSWORLD_VERSION must be v1 or v2" >&2; exit 2 ;; esac
export OSWORLD_RUNTIME_IMAGE='happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9'
```

V2.1 pins this runtime digest. V1 upstream uses a mutable image tag; this
runbook intentionally reuses the digest observed during the successful V1 run
so a reviewer rerun does not silently change the runtime.

## 3. Boot the QEMU/KVM Pod

`envsubst` substitutes only the variables listed in its first argument, leaving
the manifest itself reviewable:

```bash
envsubst '${OSWORLD_NAMESPACE} ${OSWORLD_POD} ${OSWORLD_PVC} ${OSWORLD_QCOW} ${OSWORLD_RUNTIME_IMAGE}' <<'YAML' | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: ${OSWORLD_POD}
  namespace: ${OSWORLD_NAMESPACE}
  labels:
    app.kubernetes.io/name: osworld-runtime
    app.kubernetes.io/instance: ${OSWORLD_POD}
spec:
  nodeSelector:
    kubernetes.io/hostname: liet-gpu-1
  terminationGracePeriodSeconds: 30
  containers:
    - name: qemu
      image: ${OSWORLD_RUNTIME_IMAGE}
      imagePullPolicy: IfNotPresent
      securityContext:
        privileged: true
      env:
        - name: DISK_SIZE
          value: 32G
        - name: RAM_SIZE
          value: 8G
        - name: CPU_CORES
          value: "4"
      ports:
        - { name: setup, containerPort: 5000 }
        - { name: novnc, containerPort: 8006 }
        - { name: chromium, containerPort: 9222 }
        - { name: media, containerPort: 8080 }
      resources:
        requests: { cpu: "4", memory: 8Gi }
        limits: { cpu: "8", memory: 12Gi }
      startupProbe:
        httpGet: { path: /screenshot, port: setup }
        periodSeconds: 5
        timeoutSeconds: 15
        failureThreshold: 120
      readinessProbe:
        httpGet: { path: /screenshot, port: setup }
        periodSeconds: 5
        timeoutSeconds: 15
        failureThreshold: 3
      volumeMounts:
        - name: image
          mountPath: /System.qcow2
          subPath: ${OSWORLD_QCOW}
          readOnly: true
        - name: kvm
          mountPath: /dev/kvm
  volumes:
    - name: image
      persistentVolumeClaim:
        claimName: ${OSWORLD_PVC}
    - name: kvm
      hostPath:
        path: /dev/kvm
        type: CharDevice
---
apiVersion: v1
kind: Service
metadata:
  name: ${OSWORLD_POD}
  namespace: ${OSWORLD_NAMESPACE}
spec:
  selector:
    app.kubernetes.io/instance: ${OSWORLD_POD}
  ports:
    - { name: setup, port: 5000, targetPort: setup }
    - { name: novnc, port: 8006, targetPort: novnc }
    - { name: chromium, port: 9222, targetPort: chromium }
    - { name: media, port: 8080, targetPort: media }
YAML
```

Wait for the guest, not just the container process:

```bash
kubectl -n "$OSWORLD_NAMESPACE" wait --for=condition=Ready "pod/$OSWORLD_POD" --timeout=15m
kubectl -n "$OSWORLD_NAMESPACE" logs "$OSWORLD_POD" -c qemu --tail=100
```

`Ready` can briefly turn true before an internal guest reboot. Require several
successful API checks over at least 15 seconds before uploading files, and
retry an upload if the connection resets during boot. The log should show KVM
acceleration. An exit code 88 usually means that the
container cannot open `/dev/kvm`; check the selected node and the privileged
security context.

## 4. Make the guest ports locally reachable

The qemu-docker image forwards guest ports for packets addressed to the Pod IP,
but does not bind equivalent listeners on container loopback. Consequently,
direct `kubectl port-forward pod/$OSWORLD_POD ...` fails. Run an in-cluster TCP
proxy that connects through the Service:

```bash
export OSWORLD_PROXY="${OSWORLD_POD}-proxy"
envsubst '${OSWORLD_NAMESPACE} ${OSWORLD_POD} ${OSWORLD_PROXY}' <<'YAML' | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: ${OSWORLD_PROXY}
  namespace: ${OSWORLD_NAMESPACE}
spec:
  restartPolicy: Never
  containers:
    - name: proxy
      image: alpine:3.22.1
      command: ["/bin/sh", "-ec"]
      args:
        - |
          apk add --no-cache socat
          socat TCP-LISTEN:5000,fork,reuseaddr TCP:${OSWORLD_POD}:5000 &
          socat TCP-LISTEN:8006,fork,reuseaddr TCP:${OSWORLD_POD}:8006 &
          socat TCP-LISTEN:9222,fork,reuseaddr TCP:${OSWORLD_POD}:9222 &
          socat TCP-LISTEN:8080,fork,reuseaddr TCP:${OSWORLD_POD}:8080 &
          wait
      readinessProbe:
        tcpSocket: { port: 5000 }
        periodSeconds: 2
        failureThreshold: 30
YAML
kubectl -n "$OSWORLD_NAMESPACE" wait --for=condition=Ready "pod/$OSWORLD_PROXY" --timeout=2m
kubectl -n "$OSWORLD_NAMESPACE" port-forward "pod/$OSWORLD_PROXY" 5000:5000 8006:8006 9222:9222 8080:8080
```

Keep the last command running. In another shell:

```bash
curl --fail --output /tmp/osworld-screenshot.png http://127.0.0.1:5000/screenshot
open http://127.0.0.1:8006/
```

The browser URL is the noVNC view. A single non-empty PNG confirms only a
momentary API response; allow the guest to finish rebooting before setup.

## 5. Upload the Ubuntu 22.04 AUV build

For a current-PR experiment, first build from an exact source commit in an
Ubuntu 22.04 x86_64 Pod, then use the resulting binary in the upload steps
below. Do not substitute the retained 0.0.27 binary. The Task099 blinded
pilot validated commit `25e23205`, binary SHA256
`2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`,
and maximum required glibc 2.35. These hashes identify that experiment, not
a moving `main` or PR head.

The validated build used `ubuntu:22.04` on `liet-gpu-1`, with a **task-owned**
40 GiB `tns-iscsi` PVC mounted at `/build`. Request 4 CPU, 8 GiB memory, and
2 GiB ephemeral storage; limit 8 CPU, 16 GiB memory, and 8 GiB ephemeral
storage. Put source, Cargo home, target, temporary files, and downloaded
headers on that PVC. Archive an exact clean Git commit, hash the archive,
and recheck that hash after copying it into the Pod before extracting to
`/build/src`. The task-owned build Pod/PVC should be removed after the
binary is copied and its SHA verified. Do not alter the retained
`auv-osworld-workspace` or OSWorld image PVCs.

Inside the build Pod, install the system dependencies and minimal Rust 1.95.0
toolchain. The validated dependency set was `pkg-config libclang-dev
libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev
libxkbcommon-dev libegl-dev libgbm-dev libleptonica-dev libtesseract-dev
build-essential curl ca-certificates`. Jammy's own PipeWire 0.3.48 SPA
headers do not compile this source. The scoped validation workaround was to
extract newer Debian `libspa-0.2-dev_1.4.2-1_amd64.deb` (SHA256
`7d8d46d5a98a031373d01eb74c2a1e40152294bbcaf6fb9320f88648cfde44bd`)
and `libpipewire-0.3-dev_1.4.2-1_amd64.deb` (SHA256
`6f7f4c555b6ce362ff556e7f063f92e878510564de434d56a0ded73788e83bd1`)
under `/build/headers`, not to replace Jammy's runtime libraries. The
validated source URLs were
`https://deb.debian.org/debian/pool/main/p/pipewire/libspa-0.2-dev_1.4.2-1_amd64.deb`
and
`https://deb.debian.org/debian/pool/main/p/pipewire/libpipewire-0.3-dev_1.4.2-1_amd64.deb`;
verify each downloaded file's SHA before `dpkg-deb -x`. In their
extracted `.pc` files, set `prefix=/build/headers/usr`; in
`libpipewire-0.3.pc`, keep `libdir=/usr/lib/x86_64-linux-gnu` for the Jammy
library. Verify `pkg-config --cflags --libs libpipewire-0.3` includes both
`/build/headers/usr/include/pipewire-0.3` and
`/build/headers/usr/include/spa-0.2`, and links `-lpipewire-0.3`.

The successful build invocation from `/build/src` was:

```bash
RUSTUP_HOME=/build/rustup CARGO_HOME=/build/cargo \
  CARGO_TARGET_DIR=/build/target TMPDIR=/build/tmp CARGO_BUILD_JOBS=4 \
  PKG_CONFIG_PATH=/build/headers/usr/lib/x86_64-linux-gnu/pkgconfig \
  /build/cargo/bin/cargo build -p auv-cli --bin auv --release --locked
sha256sum /build/target/release/auv
ldd /build/target/release/auv
readelf -V /build/target/release/auv
```

`auv-cli` owns the `auv` binary; `-p auv` is the wrong package. Check that
`ldd` has no missing libraries and that the maximum `GLIBC_*` requirement
does not exceed the guest's 2.35. The newer-header/Jammy-library mix is a
temporary X11 validation workaround, **not** a supported release build or
ABI policy. `rust:1.95.0-jammy` did not exist, and a build Pod on
`neko-gpu-1` without an ephemeral-storage request was Evicted during apt
installation; neither should be treated as an AUV compile failure.

For a current-head run, copy `/build/target/release/auv` out of the task-owned
build Pod to `/tmp/auv-ubuntu2204` and verify its SHA locally against the
Pod output before uploading it. The following command is **only** for
reproducing the older 0.0.27 infrastructure control from the retained
workspace Pod:

```bash
kubectl -n "$OSWORLD_NAMESPACE" exec auv-osworld-x11 -c desktop -- cat /workspace/target-ubuntu2204-v2/release/auv > /tmp/auv-ubuntu2204
shasum -a 256 /tmp/auv-ubuntu2204
```

The older digest must match the value in the inventory section; a new build
must match its own recorded digest. Uploading to the QEMU container with
`kubectl cp` would not reach the guest. Use the guest setup API:

```bash
curl --fail-with-body -F 'file_path=/home/user/auv' -F 'file_data=@/tmp/auv-ubuntu2204' http://127.0.0.1:5000/setup/upload
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["mkdir","-p","/home/user/.local/share/auv-osworld"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["chmod","0700","/home/user/auv"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["sha256sum","/home/user/auv"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq -r .output
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["/home/user/auv","--version"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq
```

The validation guest also needed `libtesseract4`, `liblept5`, and
`tesseract-ocr-eng`. Install them if `auv --version` reports a missing shared
library, then recheck the version. V1 uses sudo password `password`; V2.1 uses
`osworld-public-evaluation`. On a fresh V1 boot, `packagekitd` may briefly
hold the apt lock; wait and retry instead of killing it.

Choose either section 6A or 6B for an episode. If changing topology without
recreating the runtime Pod, stop the old guest daemon first so it does not keep
the Unix socket or TCP port.

Current `auv serve` uses `--no-register` for a temporary daemon that does not
replace the user's default local registration; older builds used
`--no-discovery`. Every HTTP listener, including loopback, now requires a
paired Device bearer. The explicit Unix listener below is the owner channel
that can create the first pairing token. The commands in sections 6A/6B were
originally live-validated with the older binary and must be rerun with a build
containing this listener-authentication change before claiming a fresh guest
pass.

## 6A. Run AUV entirely inside the guest

This is the non-paired topology. Launch the daemon with the guest X11 display
and a guest Unix socket:

```bash
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["env","DISPLAY=:0","XDG_SESSION_TYPE=x11","/home/user/auv","serve","--listen","unix:///home/user/auv.sock","--store-root","/home/user/.local/share/auv-osworld","--no-register"],"shell":false}' http://127.0.0.1:5000/setup/launch
```

Use `/setup/execute` only to start the installed AUV client. Screenshot and
input delivery still go through AUV:

```bash
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["env","DISPLAY=:0","XDG_SESSION_TYPE=x11","AUV_ENDPOINT=unix:///home/user/auv.sock","/home/user/auv","invoke","display.list","--json"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq -r .output
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["env","DISPLAY=:0","XDG_SESSION_TYPE=x11","AUV_ENDPOINT=unix:///home/user/auv.sock","/home/user/auv","invoke","display.capture","--json"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq -r .output
```

Use the same wrapper for typed input operations such as:

```text
/home/user/auv invoke input.clickPoint X Y --json
/home/user/auv invoke input.pointerPosition --json
/home/user/auv invoke input.typeText TEXT --json
/home/user/auv invoke input.keys control q --json
/home/user/auv invoke input.scrollPoint X Y DX DY --json
/home/user/auv invoke input.drag X1 Y1 X2 Y2 --duration-ms 400 --json
```

Each call needs the `DISPLAY`, `XDG_SESSION_TYPE`, and `AUV_ENDPOINT`
environment variables shown above.

## 6B. Pair a Mac AUV client to the guest

Use this topology when the decision loop runs on the Mac. Port 8080 is one of
the guest ports already forwarded by qemu-docker. Verify that it is available
inside the guest before replacing the local-only daemon with an HTTP listener:

```bash
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["sh","-lc","ss -ltnp | grep :8080 || true"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq -r .output
```

If another process owns 8080, stop that non-benchmark media service first or
keep using guest-local mode. Then launch AUV:

```bash
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["env","DISPLAY=:0","XDG_SESSION_TYPE=x11","/home/user/auv","serve","--listen","unix:///home/user/auv.sock","--listen","http://0.0.0.0:8080","--pairing-store","/home/user/.local/share/auv-osworld/pairings.json","--store-root","/home/user/.local/share/auv-osworld","--no-register"],"shell":false}' http://127.0.0.1:5000/setup/launch
```

Create the short-lived token through the owner-authorized Unix socket:

```bash
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["env","AUV_ENDPOINT=unix:///home/user/auv.sock","/home/user/auv","devices","pair","create-token"],"shell":false}' http://127.0.0.1:5000/setup/execute | jq -r .output
```

On the Mac, consume the printed token and save a profile:

```bash
auv devices pair --endpoint http://127.0.0.1:8080 connect --token '<TOKEN>' --label 'OSWorld guest' --profile osworld
auv devices list
auv --device '<DEVICE_NAME>' invoke display.list --json
auv --device '<DEVICE_NAME>' invoke display.capture --json
```

Client and guest AUV versions must match. A `0.0.26` client could list the
`0.0.27` Device but could not create a remote Run.

## 7. Run the validated GIMP task

The task ID is `7767eef2-56a3-4cea-8c9f-48c070c7d65b`; its instruction is
“Please help change GIMP's theme from dark to light.” Start GIMP through the
setup plane:

```bash
curl --fail-with-body -H 'Content-Type: application/json' -d '{"command":["gimp"],"shell":false}' http://127.0.0.1:5000/setup/launch
```

Use only AUV `display.capture` and typed input operations to open GIMP
Preferences, change the theme to Light, close the dialog, and send `Control+Q`
so GIMP persists `gimprc`. The noVNC window is for observation and debugging;
do not use it for input when recording an AUV-only result.

Retrieve the evaluator input:

```bash
curl --fail-with-body -X POST -d 'file_path=/home/user/.config/GIMP/2.10/gimprc' http://127.0.0.1:5000/file --output /tmp/gimprc
rg '^\(theme "Light"\)$' /tmp/gimprc
```

The `rg` command is a quick diagnostic. The official V1/V2 evaluator for this
task calls `check_config_status` with key `theme` and value `"Light"`; it returns
`1.0` when the retrieved configuration contains that exact setting. A full
benchmark runner must call the release-matched upstream evaluator rather than
substitute shell checks.

## 8. Reset between tasks

The source qcow2 is mounted read-only. The pinned qemu-docker image creates
`/boot.qcow2` as a qcow2 backing-file overlay of `/System.qcow2` on the
runtime container's writable root layer, and QEMU boots with
`-hda /boot.qcow2`. It does **not** use QEMU `-snapshot`. Deleting and
recreating the runtime Pod discards that container layer and creates a fresh
episode while preserving the hot base image:

```bash
kubectl -n "$OSWORLD_NAMESPACE" delete pod "$OSWORLD_PROXY" "$OSWORLD_POD"
kubectl -n "$OSWORLD_NAMESPACE" delete service "$OSWORLD_POD"
```

Re-run sections 3 and 4. Do not delete `osworld-v1-hot` or `osworld-v2-hot`
unless re-extraction from the cold archive is intended. A `local-path` PVC has
no second hot copy.

## 9. Prepare an upstream benchmark checkout

For V1, pin the exact commit used by the evidence record:

```bash
git clone https://github.com/xlang-ai/OSWorld.git
cd OSWorld
git checkout b138d348256078fa634fc3b73567a7337c793e6b
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
```

For V2.1, use its release tag and download the release-matched gated tasks and
assets after accepting both Hugging Face access requests:

```bash
git clone --branch osworld-v2.1 https://github.com/xlang-ai/OSWorld-V2.git
cd OSWorld-V2
uv sync --frozen
uvx --from huggingface_hub hf auth login
uv run scripts/tools/download_osworld_v2_tasks.py --benchmark-release osworld-v2.1
uv run scripts/tools/download_osworld_v2_assets.py --benchmark-release osworld-v2.1 --target-dir cache/osworld_v2_assets_v2.1
export OSWORLD_FILE_BASE_URL="$(pwd)/cache/osworld_v2_assets_v2.1"
export OSWORLD_BENCHMARK_RELEASE=osworld-v2.1
```

V2 tasks involving MailHub, CloudCRM, AWS Console, Overleaf, visa application,
or other mocked sites also require a self-hosted
`Task-Web/OSWorld-web@osworld-v2.1` deployment and a matching
`WEBSITE_HOST_SUFFIX`. A booted VM alone cannot run all 108 tasks comparably.

### Chrome-only V1 evaluator method-body pilot

For the pinned Chrome bookmark-folder task, the repository's
`evals/osworld/v1_evaluator.py` can run audited upstream setup and
`DesktopEnv.evaluate()` **method bodies** against an already booted guest.
It avoids the upstream Docker provider and the unrelated heavy Python import
tree; the host Python environment needs `requests`. It is evaluator-only, not
the complete official runner. Start from a fresh V1 overlay, retain the Pod
UID, and use the section 4 guest API port-forward. Run from the AUV checkout:

```bash
export OSWORLD_V1_CHECKOUT=/absolute/path/to/pinned/OSWorld
export OSWORLD_EPISODE_DIR="$(mktemp -d)"
export OSWORLD_CHROME_TASK=2ad9387a-65d8-4e33-ad5b-7580065a27ca
PYTHONDONTWRITEBYTECODE=1 timeout 180 python3 evals/osworld/v1_evaluator.py prepare \
  --upstream "$OSWORLD_V1_CHECKOUT" --task-id "$OSWORLD_CHROME_TASK" \
  --episode-dir "$OSWORLD_EPISODE_DIR" --endpoint http://127.0.0.1:5000
# Only AUV may observe and change the desktop. Stop its action phase by 10 min.
PYTHONDONTWRITEBYTECODE=1 timeout 180 python3 evals/osworld/v1_evaluator.py evaluate \
  --upstream "$OSWORLD_V1_CHECKOUT" --task-id "$OSWORLD_CHROME_TASK" \
  --episode-dir "$OSWORLD_EPISODE_DIR" --endpoint http://127.0.0.1:5000
```

The example uses GNU `timeout`; on macOS use `gtimeout` from coreutils or run
the evaluator in a Linux tooling container. Do not reuse an episode directory
or VM overlay for another attempt. The marker checks the task hash and API
endpoint but cannot prove that a restarted port-forward still reaches the
same Pod; verify the Pod UID across both phases. The bridge refuses a modified
upstream checkout and tasks other than the one allowlisted Chrome JSON. It
reports that the upstream setup loop returned true, not independent Chrome
readiness: confirm the fresh desktop shows Chrome before starting AUV actions.
It does not run AUV for you, retain AUV Run IDs, automate reset, or aggregate
scores. The two 2026-10-06 fresh-guest controls returned `0.0` without input
and `1.0` after AUV created `Favorites`; see the evidence note for the exact
scope and hashes. VLC and V2.1 are not supported by this bridge.

## 10. What remains manual

### Experimental single-episode phase adapter (one completed capture control)

`evals/osworld/k8s_phase_adapter.py` now builds a six-phase manifest for
`batch_runner.py` around either pinned V1 Chrome bookmark-folder or VLC
`play-and-exit` task. Its fixed
action is **one paired-AUV `display.capture` negative control**. It sends no
GUI input and does not measure agent ability or an AUV task-solving attempt;
the expected evaluator value is `0.0`. This adapter has local boundary tests
and passed a live boot with a measured backing-file overlay. Four live gates
stopped before AUV action/evaluation: first on the old overlay check, second
on the pinned launch response format, third on pairing-token output shape,
and fourth on `auv --version` exiting 127. The fourth gate's stderr length
and SHA256 exactly match the dynamic loader's missing `libtesseract.so.4`
message. The third gate's symptom was obscured by the ignored child exit code.
Each task-owned VM was cleaned up; none produced an AUV Run or batch score.
The adapter now checks the guest exit code and installs the runbook-validated
Tesseract packages in the disposable guest before its AUV version check.
The fifth live gate passed that path, pairing, and setup, then exposed a
host-side selector bug: it passed the canonical ID to `--device` (name
selector). The evaluator returned `0.0` without an AUV Run or screenshot;
this is not a completed capture control. The adapter now uses `--device-id`,
and the sixth fresh episode completed all six phases. Its AUV Run and PNG
were verified, and the pinned evaluator returned the expected raw `0.0`.
This is a capture-only infrastructure negative control, not an agent task
attempt or representative benchmark completion rate; see the evidence note.
The separately named VLC control later completed the same six-phase path
with its fixed task, a verified AUV capture of the VLC window, raw evaluator
`0.0`, and UID-safe reset. Neither episode delivered task-solving GUI input.

The operator supplies a JSON configuration with exactly these fields:
`batch_id`, `episode_id`, `namespace`, `kubeconfig`, `context`, `node`, `runtime_pod`,
`runtime_service`, `proxy_pod`, `proxy_image`, `base_pvc`,
`base_qcow_sha256`, `guest_auv_binary`, `host_auv_binary`,
`upstream_checkout`, `setup_local_port`, and `auv_local_port`, with an optional
`task_id`. Omitting `task_id` selects the already live-checked Chrome task;
the only other accepted ID is the pinned VLC `play-and-exit` task. Both have
now passed separate capture-only live controls. The three
resource names must be distinct and task-owned; `proxy_image` must be an
audited digest-pinned image containing `/bin/sh` and `socat`. A read-only,
task-owned hash Pod measured the retained V1 hot `System.qcow2` at
24,460,197,888 bytes and SHA256
`6bf667a852b3c307f61d9f09c42559351f45e0607e428b4997becf534cf4d313`;
boot still recomputes and compares the mounted file's hash. No placeholder
hash or image is supplied. The binary paths must contain the
specific validated guest and paired-Mac artifacts pinned in the adapter;
this is not a current-PR-head build. Configuration or manifest generation
fails when any of those checks is absent. The adapter also requires the
pinned clean V1 source checkout and the selected task's audited bytes.

After the operator has measured and reviewed those inputs, generate a
manifest and run it into a **new** output directory:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 evals/osworld/k8s_phase_adapter.py manifest --config /absolute/path/to/reviewed-config.json > /absolute/path/to/reviewed-manifest.json
PYTHONDONTWRITEBYTECODE=1 python3 evals/osworld/batch_runner.py --manifest /absolute/path/to/reviewed-manifest.json --output-dir /absolute/path/to/new-batch-dir
```

Boot records the fresh runtime/proxy Pod UIDs, container IDs, restart counts,
actual image digests, Service UID, and retained PVC/PV identities. It checks
read-only flags on both the PVC volume source and container mount, exact base
hash, and a live QEMU
command containing `-enable-kvm` and `-hda /boot.qcow2`. It verifies the
running qcow2 overlay's `/System.qcow2` backing path and format, that the
overlay is on the container's writable root rather than a Pod volume, that
QEMU has the overlay open, and that the runtime Pod/container identity did
not change during the audit. If those live checks fail, the adapter stops
rather than infer freshness. Its Pod probes
check TCP availability, then the adapter requires four successful non-GUI
`/terminal` responses across at least 15 seconds. It does not use the
OSWorld `/screenshot` endpoint to observe pixels. Every later
phase checks the pinned identities. Each guest API phase owns its loopback
port-forward and closes it on normal/error exit; the forward remains in the
runner's process group so its hard timeout also removes that route. The
installer uses `/setup/upload` and a small allowlist of non-GUI
`/setup/execute` calls; it never invokes AUV GUI operations through the setup
server. AUV capture runs through the paired Device and its task-owned profile
file. The evaluator uses the selected Chrome or VLC pinned method-body bridge,
not the complete upstream provider. Reset checks ownership UIDs before
deleting only recorded resources, verifies their absence and the hot PVC/PV
identity, then removes the task-owned pairing profile. Deletion goes through a
loopback `kubectl proxy` with Kubernetes `DeleteOptions.preconditions.uid`,
so the API refuses to delete a replacement object even if it appears after
the adapter's initial GET.

This local-tested slice is **not unattended-ready**. If boot fails between
Kubernetes creation and ownership-journal persistence, reset intentionally
refuses an unrecorded resource. An operator must inspect the episode labels,
compare the live UID with the task transcript, and decide manual cleanup.
If the journal or Pod identity changes, automatic reset also fails closed;
do not delete names blindly to recover. Verify the exact DELETE behavior in
a task-owned live probe before permitting unattended batches.
The runbook's section 6A examples that launch AUV `invoke` via
`/setup/execute` reproduce older manual infrastructure checks, **not** this
adapter's AUV-only action route. Do not copy them into an action phase.

Local tests, with no cluster access, are:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s evals/osworld/tests -p 'test_k8s_phase_adapter.py' -v
```

### Attended Chrome V1 task-directed gate

The benchmark-local `auv-osworld-action --interactive --context
/absolute/context.json` now accepts one strict JSONL action/capture sequence
over a single AUV Run. It is **not** the `batch_runner.py` action phase: the
runner deliberately sets stdin to `DEVNULL`. For a supervised rerun, use a
new episode ID, distinct resource names/ports, and a fresh episode directory
with the pinned `k8s_phase_adapter.py` config above. Validate the config,
then invoke its `boot`, `install`, and `setup` phases in order. After install,
write a version-1 context JSON with `kind: paired`, the observed canonical
`device_id`, `config_profile` equal to the episode ID, and the absolute
`paired-profiles.json` path. The context file contains no action array.

The interactive process requires `AUV_OSWORLD_EPISODE_DIR` and
`AUV_OSWORLD_ACTION_EVIDENCE` pointing to that episode and its
`action_evidence.json`. Keep the existing `Episode.forward(auv=True)` context
open around the foreground process; a previous direct launch without the
port-forward failed before obtaining a Run. The operator sends newline-ended
requests and waits for each JSONL response before continuing:

```json
{"seq":1,"op":"capture"}
{"seq":2,"op":"action","action":{"action_type":"HOTKEY","keys":["ctrl","shift","b"]}}
{"seq":3,"op":"capture"}
{"seq":4,"op":"action","action":{"action_type":"RIGHT_CLICK","x":400,"y":128}}
{"seq":5,"op":"capture"}
{"seq":6,"op":"action","action":{"action_type":"CLICK","x":500,"y":532}}
{"seq":7,"op":"capture"}
{"seq":8,"op":"action","action":{"action_type":"TYPING","text":"Favorites"}}
{"seq":9,"op":"capture"}
{"seq":10,"op":"action","action":{"action_type":"CLICK","x":1269,"y":638}}
{"seq":11,"op":"capture"}
{"seq":12,"op":"finish"}
```

These were the 2026-10-06 attended Chrome observations, **not** a fixed
general-purpose script. Inspect each AUV checkpoint before selecting the next
action; abort if the screenshot differs. Before Save, check that `Favorites`
is entered and the button is visible at the chosen coordinate. After Save,
check that the folder appears. On `finish`, compare the final stdout JSON
with the atomic sidecar and verify the PNG SHA256. Run the pinned `evaluate`
phase separately, then the UID-safe `reset` phase even if any earlier phase
failed. The observed raw Chrome score was `1.0`; the [evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md)
records hashes and the exact boundary. This is one attended method-body
evaluator result, not an unattended batch or OSWorld completion rate.

### Attended one-Run agent gateway canary

For a **new** exploratory Chrome→VLC agent cohort, freeze the
[two-episode declaration](2026-10-06-osworld-v1-codex-agent-cohort.md) in a
commit before booting either VM. Its fixed denominator and shared prompt/
budget policy apply to both fresh Codex sub-agents. The attended relay below
is not a `batch_runner.py` action phase because the runner closes phase stdin;
do not report its two rows as an unattended batch ledger or official OSWorld
rate.

The [first predeclared two-row attempt](2026-10-06-osworld-v1-codex-agent-cohort-evidence.md)
exposed two operator-facing constraints. A relay session has a 540-second
total window even if no single proposal has idled for 180 seconds; account
for model, image transfer, and operator-message latency before boot. Also,
each successfully forwarded action consumes its checkpoint provenance:
capture again before proposing another action, even if a click and typing
appear visually grounded in the same earlier frame. The gateway rejects
such a second action before AUV delivery. Explain this in the agent tool
contract; do not bypass the gate or retry inside a frozen cohort.

Both fresh V1 guests also had PackageKit hold `/var/lib/apt/lists/lock` during
the pinned install phase. Diagnose the exact guest PID/error first; in a
disposable task VM only, temporarily stop PackageKit as non-GUI setup if it
is the confirmed holder. A setup HTTP timeout during `apt-get install` does
not mean the guest subprocess stopped. Before any retry, check its process
state and installed packages, then resume only the same task-owned episode.

For an agent-selected task attempt, do not send unreviewed agent JSONL
directly to `auv-osworld-action`. Keep one foreground interactive child under
`agent_action_transport.py` and pass proposals through
`agent_action_gateway.py`. The gateway requires a first AUV capture before
any action, and each action must cite the latest checkpoint as
`based_on: {run_id, path, sha256}`. It verifies the PNG bytes and Rust
checkpoint/action/terminal sidecars, records receipts, and rejects stale or
out-of-order proposals. Capture and action budgets are caller-declared. The
agent selects actions; the relay must not silently substitute task-specific
ones. Use a fresh episode directory and resource names for every retry.

The checked-in paired-remote entry now owns that connection. After measuring
the host action binary SHA256 independently, run:

```bash
python3 evals/osworld/agent_action_relay.py \
  --config /absolute/fresh-episode/config.json \
  --episode-dir /absolute/fresh-episode \
  --action-binary /absolute/auv-osworld-action \
  --action-sha256 64-lowercase-hex-digits \
  --max-actions 32 --max-captures 32
```

It validates the episode's installed guest-binary evidence and paired
profile, refuses old action traces, opens only the existing AUV forward,
and emits a `ready` JSONL line with the Run ID, total/idle deadlines, and
single-use checkpoint rule. It does not run setup or evaluation, choose an
action, or create/delete Kubernetes resources.

Run the `boot → install → setup` phases above; the checked-in relay keeps
`Episode.forward(auv=True)` open around the foreground child and proposal
loop. Launch it in a PTY or another transport proven to keep stdin open:
a non-PTY `exec_command` launch closed stdin immediately in one live attempt.
Wait for `ready` and pass its `limits` and `rules` to the blind agent before
its first proposal; do not rely on a separately remembered protocol summary.
Request the first `capture` and show only its AUV PNG to the blind agent.
Relay one typed action proposal, wait for its receipt, request a
new `capture`, and repeat. The agent should explicitly `finish` or `abort`;
EOF is not a successful terminal message. The Rust action child now allows
240 seconds between requests, but its 570-second total deadline still applies.
Operator stalls and model latency count against these deadlines. A timeout
with no action receipt is infrastructure failure, not evidence of agent task
failure. Prompt-only tool restrictions do not enforce a sub-agent sandbox;
do not describe this as a fully isolated benchmark harness.

After terminal receipt, compare `agent_decisions.json` with
`action-requests.json`, `checkpoints.json`, `input-action-results.json`, and
`action_evidence.json`; verify every PNG SHA256. Then run the pinned
`evaluate` phase and UID-safe `reset` phase. Record the raw score separately
from driver delivery (`succeeded` is not semantic verification). The first
successful one-Run Codex canary is documented in the
[evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md); the new
checked-in relay separately passed a non-solving live negative control on a
fresh Chrome VM. It is not a general-purpose model connector. Guest-local
gateway, enforced tool isolation, model pinning, and a predeclared cohort
still require separate work.

### Scripted Chrome V1 one-task batch

For the fixed Chrome `Favorites` task only, `k8s_task_controller.py` now
connects the interactive AUV entry to the six-phase batch runner. It leaves
the runner's action stdin closed: the controller is the action-phase process
and opens a private foreground JSONL pipe to `auv-osworld-action`. The pinned
policy sends five typed actions and requires spatial OCR gates on AUV
checkpoint PNGs before continuing. This is a deterministic script, not an
autonomous agent. The 2026-10-06 fresh batch completed with raw pinned
evaluator score `1.0`, denominator one, no failure layers, and UID-safe
cleanup; see the [evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md).

Start with a fresh `k8s_phase_adapter.py` episode config as described above.
Set `task_id` explicitly to
`2ad9387a-65d8-4e33-ad5b-7580065a27ca`; select new DNS-label-safe
`batch_id`, `episode_id`, runtime Pod/Service/proxy names, and two unused
loopback ports. Recheck `liet-gpu-1` readiness, the hot PVC/PV identity,
image and binary hashes, and absence of the new resource names. A local
batch input has exactly five fields:

```json
{
  "batch_id": "unique-chrome-batch",
  "episode": "/absolute/path/to/new-episode-config.json",
  "action_binary": "/absolute/path/to/pinned/auv-osworld-action",
  "tesseract_binary": "/absolute/path/to/pinned/tesseract",
  "eng_traineddata": "/absolute/path/to/pinned/eng.traineddata"
}
```

The controller checks the episode/task, current policy bytes, host action
binary, Tesseract 5.5.2 executable, and English model against SHA256 pins
before manifest generation. The audited display is 1920×1080; a different
layout, missing OCR word, or failed typed delivery aborts the action rather
than guessing another input. Use a new output directory for each attempt:

```bash
PYTHONDONTWRITEBYTECODE=1 /Users/neko/.pixi/envs/pip/bin/python evals/osworld/k8s_task_controller.py manifest --batch /absolute/path/to/new-batch.json > /absolute/path/to/reviewed-manifest.json
PYTHONDONTWRITEBYTECODE=1 /Users/neko/.pixi/envs/pip/bin/python evals/osworld/batch_runner.py --manifest /absolute/path/to/reviewed-manifest.json --output-dir /absolute/path/to/new-output
```

Inspect `ledger.json` for all six phase statuses, failure layers, raw score,
Run ID, controller trace hash, final PNG hash, and cleanup report. Also
inspect `controller_decisions.json`, `checkpoints.json`, the checkpoint PNGs,
and `action_evidence.json`. On timeout, a forced kill marks AUV held-input
release unverified; do not infer remote release from local process exit.
If reset fails, inspect the task-owned labels and UID journal before any
manual deletion. Preserve the hot PVC/PV. The old attended sequence remains
useful to debug a changed Chrome layout, but its exact coordinates should
not be extended to VLC or V2.1 without a separately audited policy.

### VLC V1: attended diagnosis and fixed scripted policy

A fresh VLC V1 guest later passed one attended AUV-only gate with raw pinned
evaluator score `1.0`; the [evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md)
contains the exact action order, checkpoints, hashes, and UID cleanup.
For a supervised rerun, use the same fresh-ID/port/overlay and pinned task
requirements as the Chrome attended gate, but select task
`5ac2891a-eacd-4954-b339-98abba077adb`. Observe each AUV checkpoint before
sending the next typed action. In the validated episode, Ctrl+P opened
Simple Preferences, `All` at `(638,898)` reached a *fully redrawn* Advanced
Preferences page, search for `play and exit` exposed the Playlist tree,
selecting Playlist showed the target checkbox already unchecked, and Save
at `(1211,901)` wrote the desired state. The setup wrote
`play-and-exit=1` to disk *after* VLC started; the GUI showed its in-memory
unchecked default. Never blindly toggle the checkbox from that observation.

A different guest can show a stale page, a checked box, or changed geometry.
The fixed `k8s_vlc_task_controller.py` now requires the Advanced title **and**
new right pane, spatial search and Playlist signals, and a same-frame pixel
comparison of the target with known checked/unchecked controls. On a stale
or ambiguous redraw it takes at most three further captures, then returns
to VLC main and reopens Preferences within a three-opening bound. It never
clicks the target or Save without an observed unchecked target. The
task-solving policy deliberately leaves that observed unchecked state alone
and clicks Save: the pinned setup writes `play-and-exit=1` to disk after VLC
has already launched. The attended OFF→ON→OFF AUV captures validate the
checkbox predicate separately; the first scripted policy that repeated the
toggle failed closed on a fresh guest. See the [evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md).

A second fresh V1 guest reproduced an important stop condition: after All,
the title changed to Advanced while the right pane still showed Simple
Preferences, unchanged for more than seven seconds. That episode aborted
before target interaction and has **no evaluator score**. A title or All
radio alone is therefore not an adequate gate. Require visible right-pane
Playlist and the target label before considering the checkbox, and stop
without Save when they do not appear; see the [failure evidence](2026-10-05-osworld-kubernetes-x11-evidence.md).
A subsequent diagnostic loop in one fresh guest observed two such stale
cycles and one complete Advanced cycle, each returning to VLC main via AUV
Escape in about 12 seconds. This is useful to reproduce the symptom; it is
not a guarantee that retries recover or permission to omit the target-page
and checkbox-state checks.

For a fresh scripted VLC episode, copy the `k8s_phase_adapter.py` config
shape described above, select the pinned VLC task ID explicitly, and allocate
new DNS-label-safe Pod/Service names and two unused local ports. The VLC
batch input differs from Chrome's by one pinned crop tool:

```json
{
  "batch_id": "unique-vlc-batch",
  "episode": "/absolute/path/to/new-vlc-episode.json",
  "action_binary": "/absolute/path/to/pinned/auv-osworld-action",
  "tesseract_binary": "/absolute/path/to/pinned/tesseract",
  "eng_traineddata": "/absolute/path/to/pinned/eng.traineddata",
  "ffmpeg_binary": "/absolute/path/to/pinned/ffmpeg"
}
```

Run `k8s_vlc_task_controller.py manifest --batch /absolute/path/to/vlc-batch.json`
with the same Python-with-`requests` environment as the Chrome example,
inspect the exact task/policy/tool hashes and six phase argv entries, then
run `batch_runner.py` into a new output directory. The pinned display is
1920×1080. Changed OCR, ffmpeg, binary, screenshot layout, checkbox controls,
or policy bytes stop before or during action. Review the controller decisions,
Run sidecar, evaluator output, and UID-safe reset report. A successful fresh
VLC one-task batch and a later two-task Chrome→VLC batch are recorded in the
evidence note; neither is an autonomous-agent score.

For a predeclared two-task run, give the Chrome and VLC configs the **same**
new `batch_id`, distinct task/episode IDs, resource names and local ports.
Generate each controller's one-episode manifest, verify both identities and
policy hashes, then place the Chrome and VLC episode objects in one manifest
with `trust: "operator-audited"` and the shared `batch_id`. Inspect that the
combined `episodes` array has exactly two entries **before** invoking the
runner. The runner records both as scheduled before boot and executes them
serially, resetting each fresh overlay before the next episode. Preserve
the ledger even if either task fails; the denominator remains two.

### Guest-local current-head gate through an owner Unix socket

The 2026-10-06 V1 guest-local gate followed the Ubuntu 22.04 build recipe in
section 5 for exact source commits and SHA-verified `auv`,
`auv-osworld-action`, and the `action_entry` integration-test ELF. Build
dependencies, target, and temporary files lived on a **task-owned** build PVC.
The existing phase adapter's `install` cannot install a current-head binary:
it pins the older `25e23205` build. Reuse only its UID-audited `boot`/`reset`
for a fresh V1 overlay. A config containing the historical binary paths is
accepted for those phases solely because `load_config` requires their hashes;
it does **not** prove the newly compiled guest binaries.

Provision a temporary key-only SSH server as non-GUI infrastructure before
task execution. The OSWorld setup plane may upload inert ELF bytes/public key
and configure sshd, but **must not launch** `auv serve`, the action entry, or
an action test. Verify the guest's actual port 22 and ED25519 host fingerprint,
pin it in a task-owned `known_hosts`, and SSH through the task-owned proxy Pod
to the observed runtime Pod IP. Do not publish an Ingress or bypass host-key
checking. After SSH works, launch both the AUV daemon and harness **over SSH**;
keep all GUI input and capture on the guest owner Unix socket. For example,
with episode-specific paths/UID-checked names substituted:

```sh
ssh -i /absolute/ephemeral-key \
  -o UserKnownHostsFile=/absolute/known_hosts \
  -o StrictHostKeyChecking=yes -o HostKeyAlias=osworld-episode \
  -o 'ProxyCommand=kubectl --kubeconfig /absolute/ihome.conf --context CONTEXT -n NAMESPACE exec -i PROXY_POD -- socat STDIO TCP:RUNTIME_POD_IP:22' \
  user@osworld-episode
```

Inside that SSH session, set `DISPLAY=:0` and `XDG_SESSION_TYPE=x11`, start
the installed current-head `auv serve --listen unix:///home/user/auv.sock
--store-root /home/user/.local/share/auv-osworld --no-register`, and get the
online canonical Device ID with `auv devices list --endpoint
unix:///home/user/auv.sock --json`. Use `kind: guest-local`, that Device ID,
and `daemon_endpoint: unix:///home/user/auv.sock` in the interactive context.
Upload the release test ELF and action binary to the same guest absolute paths
embedded at test compilation; verify SHA256, `ldd`, and glibc requirements
inside the guest. Then run the ignored `action_entry` same-Runner capture,
EOF cancellation, and SIGTERM/reacquisition tests through SSH. For an attended
protocol sample, use `capture → typed MOVE_TO → capture → finish` and inspect
the atomic Run sidecar, checkpoints, original `InputActionResult`, and final
PNG SHA256. The [evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md)
records the exact passing gate and a test-only 1001-action boundary fix.

Finally stop the SSH-launched daemon, revoke the temporary key, close
forwards, run the adapter's UID-precondition `reset`, and remove only the
task-owned build Pod/PVC/PV after verifying artifact copies and UIDs. The
retained hot image PVC/PV must stay Bound. This gate proves guest-local AUV
action/Run lifecycle, **not** an OSWorld evaluator score; a same-test second
Run does not replace an independent X11 `ButtonRelease` receiver trace.

For a guest-local **agent** session, use the installed
`evals/osworld/agent_action_guest_local.py` with the three sibling files
`agent_action_relay.py`, `agent_action_gateway.py`, and
`agent_action_transport.py` in one guest directory. Copy their reviewed bytes
through the same host-key-verified SSH/SFTP path above and compare SHA256 on
both sides before execution. The guest entry does not import the Kubernetes
adapter, open a port-forward, pair a Device, call OSWorld setup/evaluator
APIs, choose an action, or read a model. Run it over SSH as the same
**unprivileged desktop UID** that owns the actual `/home/user/auv.sock`, with
one new absolute guest episode directory and explicit operator-measured
SHA256 values for both installed Linux ELFs:

```sh
python3 /home/user/auv-evals/agent_action_guest_local.py \
  --episode-dir /home/user/osworld-episode-unique \
  --action-binary /home/user/auv-osworld-action \
  --action-sha256 ACTION_ELF_SHA256 \
  --auv-binary /home/user/auv \
  --auv-sha256 AUV_ELF_SHA256 \
  --daemon-endpoint unix:///home/user/auv.sock \
  --device-id FULL_CANONICAL_DEVICE_ID \
  --max-actions 32 --max-captures 32
```

Preflight requires an owner Unix socket, rejects TCP and symlinks, and verifies
the exact canonical Device ID through the installed AUV's read-only
`devices list --endpoint ... --json` output (`source=daemon`, `local=true`,
`status=online`). The `ready` and proposal/receipt JSONL contract is the
same as the paired relay. Keep SSH stdin open until `finish` or `abort`; EOF
before a terminal receipt cancels the Run. A `capture` receipt names a PNG
path **inside the guest** and its SHA256. Copy only that AUV-produced file by
SFTP over the verified host key to a new host artifact path; compare the
host copy's SHA256 to the receipt before showing it to the agent or using its
checkpoint identity in a proposal. The copy is evidence transfer, not a
second GUI capture or input path. Host-key pinning and the host-side SFTP
destination are operator responsibilities; no AUV receipt authenticates SSH
or proves that host bytes were copied. Preserve the guest Run sidecars and
the host proposal/receipt transcript as separate evidence. This entry has
offline fake-child/Unix ownership tests only; an agent OSWorld score requires
a separate fresh-VM run and pinned evaluator.

This runbook reproduces the infrastructure and both AUV control topologies. It
does not yet provide:

- an unattended official-guest run of the complete typed action matrix;
- unattended iteration over 369 V1 or 108 V2.1 tasks;
- V2 mocked-site and GitLab deployment;
- per-task scheduling, timeout, reset, and result aggregation;
- automatic retrieval and invocation of every release-matched evaluator.

Until those pieces exist, run a small task subset, record the task ID, release,
qcow2 and runtime digest, AUV Device ID, topology, Run IDs, capture artifacts,
and official evaluator output for each episode.

For an isolated Xorg desktop with a running AUV owner-socket daemon, the
test-only public Runner action matrix can be invoked from the matching Linux
checkout. It starts independent Tk and raw `xev` receiver windows; these
observe but never inject GUI input:

```bash
DISPLAY=:99 XDG_SESSION_TYPE=x11 AUV_OSWORLD_TEST_DAEMON=unix:///tmp/auv-actions-gate/auv.sock \
  cargo test -p auv-osworld-evals --test xorg_actions -- --ignored --nocapture
```

Replace the display and socket with the task-owned fixture's actual values.
Run this only on a disposable Xorg desktop: the test moves the pointer, sends
clicks/keys/text, and creates/cleans held input through AUV. A passing action
matrix is still separate from an OSWorld task evaluator result.

### V2.1 Task099: six-phase fixed control and attended agent handoff

The scoped `evals/osworld/k8s_v2_task099_adapter.py` uses the same
UID-audited Pod/Service/overlay/forward/reset lifecycle as the V1 adapter,
but pins the V2.1 hot qcow2, source-built Ubuntu 22.04 guest AUV, Task099
class/getter, and gated image. It is not a general V2.1 task runner. Its
fixed action double-clicks the uploaded image through paired AUV and captures
the result; it does not create `position.txt`. The first six-phase live run
returned raw evaluator `0.0` with an Image Viewer screenshot and completed
UID-safe reset. See the [acceptance record](2026-10-07-osworld-v2-task099-bridge-acceptance.md).

Before boot, use a new DNS-label episode ID and unique Pod, Service, proxy,
and local port names. Copy the V1 lifecycle config fields into an episode
config, set `base_pvc` to `osworld-v2-hot` and `base_qcow_sha256` to the
measured V2 hot image, then add absolute `task_source`, `asset`, and
`action_binary` paths; measured `host_auv_sha256` and
`action_binary_sha256`; and the exact `action_source_commit`. The source
commit is operator-declared provenance: the adapter verifies binary bytes,
but cannot prove which checkout produced them. It rejects source/asset drift
and mismatched binaries before emitting a six-phase manifest. Use one Python
interpreter with `requests` for manifest and phases. For example, substitute
new absolute paths and inspect the manifest before running:

```sh
PYTHONDONTWRITEBYTECODE=1 /absolute/python-with-requests \
  evals/osworld/k8s_v2_task099_adapter.py manifest \
  --config /absolute/new-episode/config.json > /absolute/new-episode/manifest.json
PYTHONDONTWRITEBYTECODE=1 /absolute/python-with-requests \
  evals/osworld/batch_runner.py \
  --manifest /absolute/new-episode/manifest.json \
  --output-dir /absolute/new-run-directory
```

Inspect `ledger.json` for all six phase statuses, `auv.run_ids`, the
byte-verified final PNG, raw `evaluator_output.result.score`, and
`cleanup.report` with Pod/Service UIDs. Recheck the namespace read-only for
no task Pods or Services and retained V1/V2 PVCs Bound. A fixed-control
`0.0` is expected and is not an agent failure rate.

For a *separate fresh* attended agent attempt, run the adapter's `boot`,
`install`, and `setup` phases with the same sealed config SHA256 and
`AUV_OSWORLD_EPISODE_DIR`; do not run its fixed `action` phase. Use
`agent_action_relay.py --adapter v2-task099` with that config, episode
directory, the exact action ELF path/SHA, and bounded action/capture counts.
The config must be inside the episode directory so the relay can bind its
paired profile to that guest. Pass the task instruction and relay's `ready`
limits/rules to the agent, but do not expose the evaluator source or answer.
After the relay's terminal receipt, run the adapter's `evaluate` and `reset`
phases separately. Run reset even after a relay error; preserve the relay
trace and evaluator stdout. This attended relay does not enforce an AUV-only
model tool boundary, so such a run is exploratory, not an official benchmark
completion rate.

## Failure guide

| Symptom | Check |
| --- | --- |
| Pod exits with code 88 | `/dev/kvm`, `liet-gpu-1`, and `privileged: true` |
| PVC remains Pending | use `nodeSelector`, not `spec.nodeName`, for an unbound `WaitForFirstConsumer` PVC |
| Proxy Pod cannot install `socat` | Check whether its Alpine `apk` mirror is reachable; the Chrome evaluator live probe used a cached Python slim image with a task-owned stdlib TCP proxy instead. Pin a prebuilt proxy image for unattended batches. |
| `/screenshot` briefly succeeds then fails | wait for the startup probe across the guest reboot |
| screenshot succeeds directly but Pod remains NotReady | `/screenshot` can take several seconds; set probe `timeoutSeconds` above the observed latency (15 seconds in this runbook) |
| direct Pod port-forward refuses connections | use the Service-backed proxy in section 4 |
| AUV reports a newer glibc is required | use the Ubuntu 22.04 validation build, not the Debian 13 artifact |
| remote Device lists but invocation fails | make the Mac and guest AUV versions identical |
| local AUV cannot capture X11 | set `DISPLAY=:0` and `XDG_SESSION_TYPE=x11` on both daemon and client |
| task evaluator cannot find assets | pin and download the matching release assets; set `OSWORLD_FILE_BASE_URL` |

## Upstream references

- [OSWorld V1 repository and setup](https://github.com/xlang-ai/OSWorld/tree/b138d348256078fa634fc3b73567a7337c793e6b)
- [OSWorld V1 setup guide: 369 tasks](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/SETUP_GUIDELINE.md)
- [OSWorld-V2.1 release manifest: 108 tasks](https://github.com/xlang-ai/OSWorld-V2/blob/osworld-v2.1/benchmark_releases/osworld-v2.1.json)
- [OSWorld-V2.1 installation](https://github.com/xlang-ai/OSWorld-V2/tree/osworld-v2.1)
- [Official Docker/QEMU provider](https://github.com/xlang-ai/OSWorld-V2/blob/osworld-v2.1/desktop_env/providers/docker/provider.py)
