#!/usr/bin/env python3
"""Check the production MCP catalog without requesting any device access."""
import argparse
from pathlib import Path
import subprocess

from test_mcp_stdio import VERSIONS, exercise_hid_metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    for version in VERSIONS:
        exercise_hid_metadata(binary, version)
    rejected = subprocess.run([str(binary), '--mcp-stdio', '--mcp-backend=mock'],
                              input='', capture_output=True, text=True, timeout=5)
    if rejected.returncode == 0 or rejected.stdout.strip():
        raise ValueError('Production binary accepted the mock backend or contaminated stdout')
    print('Production MCP metadata and mock rejection passed; no hardware calls requested')


if __name__ == '__main__':
    main()
