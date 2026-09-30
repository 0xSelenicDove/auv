# Keep the Device daemon alive across graphical logout

Status: deferred host-lifecycle decision as of 2026-09-28. The first native release needs a reachable host while an existing user session is locked; it does not require a machine daemon to survive graphical logout. The boot-started lifecycle below belongs to a later signed-out implementation slice. See the [current implementation plan](2026-09-28-remote-device-entry-implementation-design.md).

## Historical broader-scope decision

The target Device's AUV daemon must start after the OS boots and remain reachable when no graphical user is signed in. A service manager owns that machine-level lifecycle, analogous to the kubelet's node-daemon lifecycle. The existing `auv serve` implementation remains the serving role; installation and service-manager integration are separate work. A controlled platform component performs any login-screen interaction because a machine-level listener alone does not obtain a graphical session or its input permissions.

This supports the owner's requested sign-in behavior at a running OS greeter. It excludes FileVault/LUKS and other preboot states where the OS and daemon have not started. The lifetime of the platform component, its privileges, and signed-out credential access still require design and native validation. See the [unlock research](2026-09-27-remote-device-unlock-research.md) and [Daemon terms](../../../TERMS_AND_CONCEPTS.md).
