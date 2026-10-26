#!/usr/bin/env python3
"""Network-only interoperability test; imports the supplied original SDK unchanged."""
import sys,subprocess,json,time,argparse,hashlib,pathlib
p=argparse.ArgumentParser(description="Original OpenRGB Python SDK against the actual Rust bridge, localhost only; no USB. Compile scripts/openrgb_interop_probe.rs with rustc --edition=2021 --extern serde=... --extern serde_json=... -L dependency=... first. Supply an unmodified pinned jath03/openrgb-python checkout.")
p.add_argument("--probe",required=True,help="Compiled openrgb_interop_probe executable")
p.add_argument("--sdk-source",required=True,help="Pinned upstream checkout containing openrgb/")
args=p.parse_args()
sys.path.insert(0,args.sdk_source)
from openrgb import OpenRGBClient
from openrgb.utils import RGBColor
from openrgb.network import OPENRGB_PROTOCOL_VERSION
probe=subprocess.Popen([args.probe],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
client=None
try:
 port=int(probe.stdout.readline())
 client=OpenRGBClient('127.0.0.1',port=port,name='AK820 original SDK offline interop')
 assert client.protocol_version==1
 assert len(client.devices)==1
 device=client.devices[0]
 assert len(device.leds)==82 and len(device.zones)==1
 zone=device.zones[0]
 matrix=zone.matrix_map
 assert len(matrix)==6 and all(len(row)==17 for row in matrix)
 assert sorted(v for row in matrix for v in row if v is not None)==list(range(82))
 assert matrix[0][0]==0 and matrix[0][14]==13 and matrix[5][16]==81
 device.set_mode('Direct')
 assert device.active_mode==0
 def snapshot():
  probe.stdin.write('frame\n'); probe.stdin.flush()
  line=probe.stdout.readline()
  assert line,'probe ended before frame'
  return json.loads(line)
 device.set_colors([RGBColor(11,22,33)]*82)
 assert snapshot()==[[11,22,33]]*82
 zone.set_colors([RGBColor(44,55,66)]*82)
 assert snapshot()==[[44,55,66]]*82
 device.leds[81].set_color(RGBColor(77,88,99))
 expected=[[44,55,66]]*81+[[77,88,99]]
 assert snapshot()==expected
 client.disconnect(); client=None
 time.sleep(.05)
 probe.stdin.write('disconnected\n'); probe.stdin.flush()
 assert probe.stdout.readline().strip()=='disconnected'
 probe.stdin.write('done\n'); probe.stdin.flush()
 assert probe.wait(timeout=3)==0
 print(json.dumps({'result':'PASS','client':'jath03/openrgb-python','source_sha256':hashlib.sha256(b''.join(p.relative_to(pathlib.Path(args.sdk_source)).as_posix().encode()+b'\0'+p.read_bytes() for p in sorted((pathlib.Path(args.sdk_source)/'openrgb').rglob('*.py')))).hexdigest(),'sdk_advertised':OPENRGB_PROTOCOL_VERSION,'sdk_negotiated':1,'controllers':1,'zones':1,'leds':82,'matrix':[6,17],'operations':['enumerate','set_mode Direct (1101)','whole-controller colors (1050)','zone colors (1051)','single LED (1052)','disconnect clears frame'],'usb':'not present in probe'}))
finally:
 if client: client.disconnect()
 if probe.poll() is None: probe.kill(); probe.wait()
