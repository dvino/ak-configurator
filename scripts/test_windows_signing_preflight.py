#!/usr/bin/env python3
"""Exercise CI's imported signing identity using a disposable CMS message."""
import os
from pathlib import Path

from windows_signing import sign_report, verify_report_signature


def main():
    thumbprint = os.environ['AK_WINDOWS_CODE_SIGN_THUMBPRINT']
    report = b'{"format":"ak-configurator-ci-signing-preflight-1"}\n'
    signature = sign_report(report, thumbprint)
    verify_report_signature(report, signature, Path(__file__).resolve().parents[1])
    print('Local-development CMS signing and verification passed; public trust is not asserted')


if __name__ == '__main__':
    main()
