# Bundled audio visualizer

Music visualization reuses CAVA 1.0.0 by Karl Stavestrand and contributors.
The exact source commit, archive SHA256 and local changes are recorded in
`vendor/cava-provenance.json`. Complete upstream source and its MIT license
are retained in `vendor/cava`. Release packages contain a separate hidden
`audio/cava` executable (`cava.exe` on Windows); no separate CAVA installation
is required. The helper captures only while Music is explicitly running.

CAVA performs capture, FFT, smoothing and automatic sensitivity. The application
consumes sixteen mono ASCII spectrum bars at ten frames per second. Linux uses
PulseAudio's default output monitor, including PipeWire's PulseAudio service.
The PulseAudio client runtime is a system dependency. macOS uses the public
CoreAudio process tap on macOS 14.2 or later and requires system audio recording
consent. Windows uses WASAPI loopback on the default playback endpoint.
A missing helper, unavailable service or denied permission is a visible failure.

The helper build statically links FFTW (GPL 2 or later) and iniparser (MIT) on
Unix; Windows uses vcpkg static FFTW, pthreads; unused SDL2 and GLEW are omitted. Dependency licenses
and corresponding source must accompany redistribution as required by their
licenses. Unsigned CI packages and synthetic tests do not establish OS consent,
loopback capture, CPU use or physical keyboard behavior on any platform.

Local source changes suppress terminal title escapes on nonterminal stdout,
remove private macOS TCC calls and automatic Settings launch, and fix Windows
multichannel downmix bounds before reading the sixth/eighth channel. No custom
FFT or replacement signal analysis is introduced.

Windows tooling, the manifest baseline and the explicit dependency registry all
use immutable vcpkg commit `64810c6bf2bdc91ebcd75f140aec77580113f219`.
FFTW is pinned to 3.3.10 port revision 10 and PThreads4W to 3.0.0 port revision 14.
The unused moving artifact registry is removed. The builder rejects inconsistent
registry/tool/version pins and verifies that the archived port recipes match the
resolved versions. This registry correction is the seventh modified upstream
source file; all seven paths are listed in `vendor/cava-provenance.json`.

Windows CI separately prepares native pkgconf 3.0.10 from its official x64 or
arm64 MSI. The build script checks the pinned size and SHA256 before administrative
extraction, then uses its absolute `PKG_CONFIG` path and preserves that variable
through vcpkg's clean environment. This removes the stale MSYS2 download path
without changing FFTW/pthreads versions or static linkage. pkgconf is a build
tool and is not included in the application. The complete build script and
[rebuild instructions](../vendor/cava-dependencies/REBUILD.md) remain in the
package's `audio-sources` directory.

After preparing the pinned vcpkg checkout, run in PowerShell:

```powershell
$env:PKG_CONFIG = python scripts/build_cava_helper.py --prepare-windows-pkgconf build/pkgconf
if ($LASTEXITCODE -ne 0) { throw 'pkgconf preparation failed' }
python scripts/build_cava_helper.py --output-dir build/audio
```

The first command downloads and verifies the tool, extracts it into the named
directory and prints its path. It does not install pkgconf or change system PATH.
