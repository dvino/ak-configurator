# Rebuilding the bundled CAVA helper

This source kit contains the complete patched CAVA source, exact dependency
source archives and licenses, dependency hashes, and the exact compilation
script used by AK Configurator. Python 3.12 or newer is required. No command
below starts capture or accesses a keyboard.

From the extracted package root, construct the source layout expected by the
compilation script:

```sh
mkdir -p cava-rebuild/vendor/cava-dependencies cava-rebuild/scripts
cp audio-sources/build_cava_helper.py cava-rebuild/scripts/
cp audio-sources/* cava-rebuild/vendor/cava-dependencies/
tar -xzf audio-sources/cava-patched-source.tar.gz -C cava-rebuild/vendor
cd cava-rebuild
```

The relevant layout is `scripts/build_cava_helper.py`, `vendor/cava/` and
`vendor/cava-dependencies/`. Extra copied documentation/source archives are
harmless; the dependency provenance JSON names the archives compiled on Unix.

Linux requires a C compiler, make, ar and PulseAudio development headers/client
libraries (for example `build-essential libpulse-dev` on Debian/Ubuntu).
macOS requires the Xcode command line tools and the macOS 14.2+ SDK. The script
checks dependency archive hashes, builds FFTW 3.3.10 and iniparser 4.2.6 from the
included source, links both statically, and uses the OS audio client API:

```sh
python3 scripts/build_cava_helper.py --output-dir build/audio
```

Windows requires Visual Studio's C/C++ tools, CMake, Git and Python. Clone the
official vcpkg repository and check out the exact baseline below, then bootstrap
it with metrics disabled. The explicit registry in `vcpkg-configuration.json`
uses the same commit and contains no moving artifact registries. Set `VCPKG_ROOT` to its absolute path and invoke the
same script. The script rejects any other tool checkout. The CAVA manifest pins
FFTW 3.3.10 port revision 10 and PThreads4W 3.0.0 port revision 14. The builder checks registry/manifest agreement and confirms that the archived
port recipes have these exact versions. Both target
architectures use the static runtime and static dependency triplets.

Prepare native pkgconf 3.0.10 before building. The explicit preparation command
downloads the official x64 or arm64 MSI, verifies its pinned size and SHA256,
and uses `msiexec /a` to extract an administrative image under the specified
directory. It does not install pkgconf or change PATH. The printed executable
path becomes `PKG_CONFIG`; the builder also preserves it through vcpkg's clean
environment using `VCPKG_KEEP_ENV_VARS`. This avoids vcpkg's obsolete MSYS2
package downloads. A missing or wrong-version tool stops before CMake.

```powershell
git clone https://github.com/microsoft/vcpkg.git vcpkg
git -C vcpkg checkout 64810c6bf2bdc91ebcd75f140aec77580113f219
./vcpkg/bootstrap-vcpkg.bat -disableMetrics
if ($LASTEXITCODE -ne 0) { throw 'vcpkg bootstrap failed' }
$env:VCPKG_ROOT = (Resolve-Path ./vcpkg).Path
$env:PKG_CONFIG = python scripts/build_cava_helper.py --prepare-windows-pkgconf build/pkgconf
if ($LASTEXITCODE -ne 0) { throw 'pkgconf preparation failed' }
python scripts/build_cava_helper.py --output-dir build/audio
```

The package additionally includes `windows-built-dependencies.tar.gz` on Windows.
It retains the actual patched vcpkg dependency sources, port recipes and licenses
from that native build. The pristine source archives are also supplied here.
The current Windows recipe obtains its verified sources through vcpkg; a network
connection is needed for the first tool/dependency setup. Unix compilation uses
the bundled archives without downloading dependency sources.

The result is `build/audio/cava` or `build/audio/cava.exe`. Place that executable
beside AK Configurator as `audio/cava` or `audio/cava.exe`; macOS uses
`AK-Configurator.app/Contents/MacOS/audio/cava`. `cava -v` prints its version and
exits without capture. Compilation on a target does not establish its OS audio
permissions, installed runtime capture or physical keyboard behavior.
