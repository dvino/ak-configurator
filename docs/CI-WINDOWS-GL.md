Русский перевод: [Windows CI graphics](ru/CI-WINDOWS-GL.md).

# Windows CI graphics fixture

The retained native smoke reports from run `40538179782` show the same failure on Windows x86_64 and ARM64: `egui_glow requires opengl 2.0+`. No runtime scenario completed. This identifies a renderer initialization failure on those hosted runners, not a keyboard fault.

The candidate wrapper `scripts/run_windows_ci_runtime_smoke.py` supplies Mesa llvmpipe only to the Windows CI smoke. It keeps the production Glow/WGL renderer, native window and tray scenarios, offline application mode, deadlines and failure gates. Preparation downloads a pinned graphics archive; the application runs through the existing isolated `--runtime-smoke` harness. This is not an OS network/device sandbox.

The third-party distribution is [mmozeiko/build-mesa 26.2.5](https://github.com/mmozeiko/build-mesa/releases/tag/26.2.5), source commit `cb4cc458eff7c4b4b31d2623edd954d66201b88f`. It is not an official Mesa binary release. The [distribution build script](https://github.com/mmozeiko/build-mesa/blob/cb4cc458eff7c4b4b31d2623edd954d66201b88f/build.cmd) selects native AArch64/X86 LLVM targets and static dependencies. [Mesa documentation](https://docs.mesa3d.org/drivers/llvmpipe.html) describes adjacent Windows WGL DLL deployment.

| Native target | Release asset | SHA-256 |
| --- | --- | --- |
| ARM64 | `mesa-llvmpipe-arm64-26.2.5.7z` | `e7df75398d50e07a68bbffa5cff2ece4c85e5f8ef8bda497f560dd04fe60cbbd` |
| x86_64 | `mesa-llvmpipe-x64-26.2.5.7z` | `d1b713ca2a7fba47a50c058099f13c435ddb76a16a422c334e4d787f9755a31d` |

Both downloaded archive hashes were checked locally. Each archive contains root `opengl32.dll`, `libEGL.dll`, `libGLESv1_CM.dll`, `libGLESv2.dll` and three EGL/GLES import libraries. Only the exact root `opengl32.dll` member streams through 7-Zip stdout into a fixed file; no archive pathname is used as an extraction destination. Download and DLL sizes are bounded, and PE machine type must match the native host. A reader thread enforces the 128 MiB output cap while the main thread enforces a 30-second extraction deadline, including a child that stalls before stdout EOF. Timeout kills and reaps the child before cleanup.

A private directory under `RUNNER_TEMP` holds the archive, DLL and copied production executable. SHA-256 of the original, temporary copy and smoke report must agree. The directory is removed after success or failure; package paths are never modified. The report adds archive URL/hash, source commit, DLL hash, executable hashes and CI-only software graphics scope. The requested Mesa driver is labeled separately from the production application renderer. It explicitly does not independently prove which DLL the process loaded. Failed smoke reports retain the existing separate diagnostic upload; packaging and candidate uploads still require success.

Local tests cover pinning, malformed or modified inputs, architecture mismatch, exact member selection, size limits, isolation, cleanup and preserved smoke failure. A real sleeping child verifies timeout, termination, failed-report retention and temporary-directory cleanup without launching the application. They do not launch Windows. An independent review found a stalled extraction could bypass the deadline; the correction and its regression test were reviewed again with no remaining findings. Ten fixture tests and two workflow tests passed locally. Actual Windows CI rendering and final-source validation remain required. A later software rendering pass would not establish a hardware GPU, physical tray click, normal-desktop driver compatibility or keyboard qualification. The DLL is not included in application packages.
