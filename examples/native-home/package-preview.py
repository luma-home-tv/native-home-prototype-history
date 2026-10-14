"""Package only the isolated native preview; no services, hooks or system overlays."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

here = Path(__file__).resolve().parent
repo = here.parents[1]
output = Path(os.environ['NATIVE_HOME_BUILD_DIR']).resolve()
sdk = Path(os.environ['NATIVE_HOME_WEBOS_SDK']).resolve()
binary = Path(os.environ['NATIVE_HOME_ARM_BINARY']).resolve()
main = binary.name
app_id = 'com.ash.nativehome.targetproof' if main == 'launch-target' else 'com.ash.nativehome.preview'
stage = output / 'preview-package/usr/palm/applications' / app_id
stage.mkdir(parents=True, exist_ok=False)
if main not in ('control-preview', 'home-preview', 'launch-target'): raise ValueError('Unexpected preview binary')
version = '0.6.0' if main == 'home-preview' else '0.1.0'
shutil.copy2(binary, stage / main)
(stage / main).chmod(0o755)
for name in ('appfont.ttf', 'appfont-bold.ttf', 'OFL.txt'):
    shutil.copy2(repo / 'pkg' / name, stage / name)
# Independent Luma Home branding; no PlxNative mark is used as this client's identity.
icon = Path(os.environ.get('NATIVE_HOME_PREVIEW_ICON', repo / 'assets/luma-home/icon-160.png')).resolve()
shutil.copy2(icon, stage / 'icon.png')
info = {'id': app_id, 'version': version, 'vendor': 'Native Home development',
        'type': 'native', 'main': main, 'title': 'Native Launch Target' if main == 'launch-target' else 'Luma Home Preview',
        'appDescription': 'Isolated native interface preview; no TV actions or settings writes',
        'icon': 'icon.png', 'largeIcon': 'icon.png', 'transparent': False,
        'nativeLifeCycleInterfaceVersion': 2, 'handlesRelaunch': False,
        'requiredMemory': 96, 'requiredPermissions': []}
(stage / 'appinfo.json').write_text(json.dumps(info, indent=2) + '\n')
if main == 'home-preview':
    assets = Path(os.environ['NATIVE_HOME_SNAPSHOT_ASSETS']).resolve()
    snapshot = json.loads((assets / 'snapshot.json').read_text())
    shutil.copy2(assets / 'snapshot.json', stage / 'snapshot.json')
    if os.environ.get('NATIVE_HOME_LAUNCH_PROOF') == '1':
        (stage / 'launch-proof.json').write_text(json.dumps({'kind':'Application','id':'com.ash.nativehome.targetproof'})+'\n')
    route = os.environ.get('NATIVE_HOME_ROUTE_PROOF')
    if route:
        if route not in ('settings', 'hdmi4'): raise ValueError('Unknown proof route')
        proof = {'kind':'LgSettings'} if route == 'settings' else {'kind':'Input','id':'HDMI_4'}
        (stage / 'launch-proof.json').write_text(json.dumps(proof)+'\n')
    if os.environ.get('NATIVE_HOME_PREFERENCES_PROBE') == '1':
        shutil.copy2(assets / 'preferences-hold-probe.json', stage / 'preferences-hold-probe.json')
    if os.environ.get('NATIVE_HOME_NATIVE_READ_PROBE') == '1':
        (stage / 'native-read-probe.json').write_text('{}\n')
    for app in snapshot['applications']:
        icon_path = app.get('icon')
        if not icon_path: continue
        relative = Path(icon_path)
        if relative.is_absolute() or len(relative.parts) != 2 or relative.parts[0] != 'icons' or '..' in relative.parts:
            raise ValueError('Unexpected icon path')
        source = assets / relative
        if source.is_symlink() or source.stat().st_size > 1024 * 1024:
            raise ValueError('Unsafe icon asset')
        (stage / 'icons').mkdir(exist_ok=True)
        shutil.copy2(source, stage / relative)

control = ('Package: ' + app_id + '\nVersion: ' + version + '\nSection: misc\nPriority: optional\n'
           'Architecture: arm\nMaintainer: Native Home development\n'
           'Description: Isolated native interface preview\n').encode()
with tarfile.open(output / 'control.tar.gz', 'w:gz', format=tarfile.USTAR_FORMAT) as tar:
    entry = tarfile.TarInfo('./control'); entry.size = len(control); entry.mode = 0o644
    entry.uid = entry.gid = 0; entry.mtime = 1700000000
    tar.addfile(entry, io.BytesIO(control))
with tarfile.open(output / 'data.tar.gz', 'w:gz', format=tarfile.USTAR_FORMAT) as tar:
    for path in sorted((output / 'preview-package').rglob('*')):
        entry = tar.gettarinfo(str(path), arcname='./' + str(path.relative_to(output / 'preview-package')))
        entry.uid = entry.gid = 0; entry.uname = entry.gname = 'root'; entry.mtime = 1700000000
        if path.is_file():
            with path.open('rb') as stream: tar.addfile(entry, stream)
        else: tar.addfile(entry)
(output / 'debian-binary').write_text('2.0\n')
ipk = output / (app_id + '_' + version + '_arm.ipk')
ipk.unlink(missing_ok=True)
ar = sdk / 'bin/arm-webos-linux-gnueabi-ar'
subprocess.run([str(ar), 'rc', str(ipk), 'debian-binary', 'control.tar.gz', 'data.tar.gz'], cwd=output, check=True)
members = subprocess.check_output([str(ar), 't', str(ipk)], text=True).splitlines()
if members != ['debian-binary', 'control.tar.gz', 'data.tar.gz']: raise RuntimeError('Invalid IPK members')
receipt = {'app_id': app_id, 'ipk': str(ipk), 'sha256': hashlib.sha256(ipk.read_bytes()).hexdigest(),
           'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'bytes': ipk.stat().st_size,
           'preference_hold_probe': os.environ.get('NATIVE_HOME_PREFERENCES_PROBE') == '1',
           'native_read_probe': os.environ.get('NATIVE_HOME_NATIVE_READ_PROBE') == '1',
           'launch_proof': os.environ.get('NATIVE_HOME_LAUNCH_PROOF') == '1',
           'route_proof': os.environ.get('NATIVE_HOME_ROUTE_PROOF'),
           'permissions': [], 'services': [], 'boot_hooks': [], 'system_files': []}
(output / 'preview-package-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
