# Enroll the OS login credential on the target Device

Status: accepted target-local credential policy; scope revised on 2026-09-28. The first native release unlocks existing locked sessions only. Its installed host must retrieve an enrolled credential while that session is locked; retrieval before any user logs in and the signed-out sign-in consequences below are deferred. See the [current implementation plan](2026-09-28-remote-device-entry-implementation-design.md).

## Linux locked-session rotation rule, accepted 2026-09-29

For the Debian GNOME locked-session path, revalidate the protected stored OS
password against the target's installed `gdm-password` PAM service before
**every** remote unlock attempt, including an enrollment previously marked
`READY`. logind's `Unlock` signal does not consume password bytes. If PAM
confirms rejection, suspend that enrollment generation and do not send
`Unlock`; the owner must re-enroll locally after a password change. If PAM or
the Secret Service is unavailable, fail the attempt without suspending the
enrollment because credential rejection was not established. The password
stays target-local, and neither PAM messages nor password bytes enter the
paired response, trace, or audit. This rule is specific to the password-only
PAM policy on the tested host; other Linux authentication factors remain
outside this candidate.

## Historical broader-scope decision

To make an OS account eligible for remote desktop entry, AUV must store a separately enrolled copy of that user's usable OS login credential on the target Device. Enrollment is incomplete without the credential, even on a platform that could unlock an existing session without entering one; a lock-only enrollment mode is excluded. A target-local OS administrator may enroll any OS account; separate confirmation by that account holder is not required. A non-administrator may also enroll their own OS account locally, but not another user's account. The local enrollment interface must determine the caller's OS identity rather than trusting a requested username as proof of ownership. The credential is supplied locally during enrollment; remote unlock requests do not carry it. Enrollment must verify that the actual machine-level service identity can retrieve it before reporting readiness. Prefer a system-level protected keystore or keychain that remains available before the user logs in. If no suitable protected backend exists, a local administrator may explicitly select a plaintext credential file; AUV must not silently fall back to one, and a paired remote caller cannot enable that fallback. The concrete protected backends, file access rules, and rotation behavior remain open.

This is a product trade-off: unattended target-local sign-in needs a credential accessible without an already logged-in user, while ordinary user keychains and Windows Hello material cannot be assumed readable by the machine service. The live macOS and Windows probes held a secret in a process only until unlock; they did not validate persistent secret storage or signed-out login. See the [unlock research](2026-09-27-remote-device-unlock-research.md) and [Device terms](../../../TERMS_AND_CONCEPTS.md).

If the OS clearly rejects a stored credential during a remote entry attempt, AUV suspends remote entry for that OS account. Later paired requests must not resubmit the same credential; the account becomes eligible again only after local re-enrollment with a usable credential. An input-delivery failure or an unverified outcome is not, by itself, proof that the credential was rejected. The exact platform signal for confirmed rejection and how to handle an indeterminate attempt remain implementation questions.

Credential deletion is a separate target-local operation from disabling the machine-wide remote-entry switch. An ordinary user may delete their own account's enrollment; a target-local OS administrator may delete any account's enrollment. The local interface must determine the caller's OS identity. Once deletion completes, subsequent remote requests for that account cannot unlock or sign in until it is enrolled again. Whether an already in-flight attempt can be stopped depends on when native credential delivery began and remains an implementation question.
