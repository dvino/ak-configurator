#!/usr/bin/env python3
"""Offline real-helper test using synthetic PCM only; never opens system capture."""
import argparse
import math
import os
from pathlib import Path
import selectors
import signal
import struct
import subprocess
import tempfile
import threading
import time


def verify(binary):
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        fifo = root / 'pcm'
        os.mkfifo(fifo)
        config = root / 'config'
        config.write_text(f'''[general]
bars = 16
framerate = 10
[input]
method = fifo
source = {fifo}
sample_rate = 44100
sample_bits = 16
[output]
method = raw
raw_target = /dev/stdout
data_format = ascii
ascii_max_range = 255
bar_delimiter = 59
frame_delimiter = 10
channels = mono
mono_option = average
[smoothing]
noise_reduction = 0.1
''')
        process = subprocess.Popen([str(Path(binary).resolve()), '-p', str(config)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env={**os.environ, 'XDG_CONFIG_HOME': str(root / 'config-home')})
        stop = threading.Event()
        silence = threading.Event()
        def write_pcm():
            fd = os.open(fifo, os.O_RDWR | os.O_NONBLOCK)
            phase = 0
            try:
                while not stop.is_set():
                    samples = [(0 if silence.is_set() else int(24000 * math.sin(2 * math.pi * 1000 * (phase + i) / 44100))) for i in range(441)]
                    data = b''.join(struct.pack('<hh', value, value) for value in samples)
                    try:
                        os.write(fd, data)
                        phase += 441
                    except BlockingIOError:
                        pass
                    stop.wait(0.01)
            finally:
                os.close(fd)
        thread = threading.Thread(target=write_pcm, daemon=True)
        thread.start()
        frames = []
        buffer = b''
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        deadline = time.monotonic() + 8
        try:
            while time.monotonic() < deadline and len(frames) < 24:
                if not selector.select(0.2):
                    if process.poll() is not None:
                        raise AssertionError(process.stderr.read().decode(errors='replace'))
                    continue
                buffer += os.read(process.stdout.fileno(), 4096)
                while b'\n' in buffer:
                    line, buffer = buffer.split(b'\n', 1)
                    fields = line.rstrip(b';').split(b';')
                    assert len(fields) == 16, repr(line)
                    values = [int(value) for value in fields]
                    assert all(0 <= value <= 255 for value in values)
                    frames.append(values)
                    if len(frames) == 8:
                        silence.set()
            assert len(frames) >= 5, 'too few bounded raw frames'
            assert any(max(frame) > 0 for frame in frames[:8]), 'synthetic sine produced no spectrum'
            assert len(frames) >= 20 and any(max(frame) == 0 for frame in frames[-4:]), 'synthetic silence did not decay to zero'
        finally:
            stop.set()
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
            thread.join(timeout=2)
            selector.close()
        return {'frames': len(frames), 'bars': 16, 'capture': 'synthetic sine and silence FIFO only', 'status': 'passed'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    print(verify(parser.parse_args().binary))
