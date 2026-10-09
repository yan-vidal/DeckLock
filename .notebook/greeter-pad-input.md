# Greeter pad input
> Opt-in daemon ownership; root checkout drafts reconciled against 0.3.3

Entry: `src/setup.rs:setup_greeter_with_storage()` → generated `--greeter-controller`
Runtime: `src/main.rs:run()` → `src/greeter_controller.rs:GreeterController::start()`
Input: `src/controller.rs:ControllerClient` → `src/ui.rs:build_keyboard()`

- Real greetd only; explicit preview/absent GREETD_SOCK never starts a daemon.
- sc-controller installed locally: 0.7.1. `scc/bin/scc_daemon.py:main()` accepts a
  profile path, `start`, `--foreground`, `--alone`. No launcher override is needed.
- Old prototype's `Profile()` fails: installed constructor requires `ActionParser`.
  Pure local parsing of the embedded profile uses `Profile(ActionParser())`, no daemon.
- Private runtime HOME/XDG; no saved SCC profiles or PID registration. Static profile
  has no shell actions, no absolute machine paths and no second OSK.
- Normal Drop requests SIGTERM, escalates within a bound and reaps. Parent-death
  SIGKILL covers crashes and abrupt termination. No main signal handler/unlock change.
- Abrupt death may leave static runtime files until runtime cleanup; no passwords.
- GTK pad picking uses allocated button bounds: after showing a hidden keyboard,
  tests must wait for allocation; fixed keyboard layout avoids host XKB dependence.

Reconciliation baseline: main `26e1bef` (v0.3.3), root old branch `7bc3f97`.
- Old workflow/documentation aggregation draft superseded by main's ten required jobs.
- Old staged UID/runtime environment workaround not ported: greetd/logind manage it;
  real system-service VM tests already establish the current path. It also forced
  session type rather than using the chosen session's environment.
- Root tracked changes preserved in named stash `reconciled-2026-10-09-pre-greeter-pad-input`.
  Object: `40cb86326547b03c9763d60199a1cfb6bd335648`; restore on the old branch,
  with `git stash apply --index 40cb86326547b03c9763d60199a1cfb6bd335648` if needed.
- Exact patches/status and original untracked prototype copied to
  `target/reconciliation-2026-10-09/`; originals retained. `.deps`/old notebook retained.
- Worktree name remains `controller-reconnect`, but its branch is `feat/greeter-pad-input`.

Protocol finding: installed `SCCDaemon:_sshandler()` sends `Controller Count: 0`
then `Ready.` without hardware; `add_controller()` sends a list without a new
Ready. `src/controller.rs:run()` must keep fallback on the first sequence and
emit Connected for hotplug after Ready. Disconnect clears pending selection/capture.

Evidence: `tests/cli_contract.rs:greeter_pad_setup_is_explicit_and_forwards_to_both_compositors()`;
`scripts/test-greeter-controller.py` (real binary/fake daemon/private GTK);
`examples/preview_check.rs` (greeter pad typing/disconnect fallback).
Device/package boundaries: `docs/greeter-controller-uat.md`; no hardware or live-service test.

Updated: 2026-10-09
