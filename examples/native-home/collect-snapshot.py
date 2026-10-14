"""Read-only TV snapshot and bounded local icon copies for the isolated Home renderer.
Nothing is installed, launched, set or written on the TV. Captures are private build artifacts.
"""
import argparse
import datetime
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import subprocess


def app_id(value):
    return isinstance(value, str) and bool(re.fullmatch(r'[A-Za-z0-9._-]{1,180}', value))


def icon_path(folder, image, identity):
    prefixes = ('/usr/palm/applications', '/media/cryptofs/apps/usr/palm/applications',
                '/media/developer/apps/usr/palm/applications')
    if not app_id(identity) or folder not in [p + '/' + identity for p in prefixes]:
        return None
    if not isinstance(image, str) or '\x00' in image:
        return None
    path = PurePosixPath(image)
    if '..' in path.parts:
        return None
    if not path.is_absolute():
        path = PurePosixPath(folder) / path
    return str(path) if str(path).startswith(folder + '/') and path.suffix.lower() in ('.png', '.jpg', '.jpeg') else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--identity', type=Path, required=True)
    parser.add_argument('--known-hosts', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9.:-]+', args.host):
        parser.error('Invalid host')
    output = args.out.resolve(); output.mkdir(mode=0o700, parents=True, exist_ok=False)
    (output / 'icons').mkdir(mode=0o700)
    base = ['ssh', '-i', str(args.identity.resolve()), '-o', 'IdentitiesOnly=yes', '-o', 'BatchMode=yes',
            '-o', 'StrictHostKeyChecking=yes', '-o', 'UserKnownHostsFile=' + str(args.known_hosts.resolve()),
            '-o', 'ConnectTimeout=5', 'root@' + args.host]
    receipts = {}
    def luna(name, uri, payload):
        command = 'luna-send -i -n 1 -w 5000 -f luna://' + uri + ' ' + shlex.quote(json.dumps(payload))
        result = subprocess.run(base[:-1] + ['-tt', base[-1], command], capture_output=True, timeout=12, check=True)
        data = json.loads(result.stdout)
        receipts[name] = data
        return data
    apps = luna('apps', 'com.webos.applicationManager/listApps', {}).get('apps', [])
    points = luna('launch_points', 'com.webos.applicationManager/listLaunchPoints', {}).get('launchPoints', [])
    inputs = luna('inputs', 'com.webos.service.eim/getAllInputStatus', {}).get('devices', [])
    picture = luna('picture', 'com.webos.service.settings/getSystemSettings', {'category': 'picture', 'keys': ['pictureMode', 'backlight', 'contrast', 'brightness']})
    sound = luna('audio', 'com.webos.service.audio/master/getVolume', {})
    ids = {p['id'] for p in points if app_id(p.get('id'))}
    point_by_id = {p['id']: p for p in points if p.get('id') in ids}
    applications = []
    for app in apps:
        identity = app.get('id')
        if not app_id(identity) or identity not in ids or app.get('visible') is not True or app.get('type') not in ('native', 'native_builtin', 'web'):
            continue
        item = {key: app[key] for key in ('id', 'title', 'type', 'visible')}
        folder = app.get('folderPath')
        point = point_by_id[identity]
        candidates = [point.get('extraLargeIcon'), point.get('largeIcon'), app.get('icon')]
        if len(applications) < 64:
            for candidate in candidates:
                path = icon_path(folder, candidate, identity)
                if not path: continue
                # Fixed, bounded read. Values are paths from registered metadata, shell-quoted.
                quoted = shlex.quote(path)
                cmd = 'test -f '+quoted+' && test ! -L '+quoted+' && test "$(stat -c %s '+quoted+')" -le 1048576 && head -c 1048576 '+quoted
                result = subprocess.run(base + [cmd], capture_output=True, timeout=8)
                data = result.stdout
                if result.returncode or not (data.startswith(b'\x89PNG\r\n\x1a\n') or data.startswith(b'\xff\xd8\xff')): continue
                name = f'icons/{identity}' + PurePosixPath(path).suffix.lower()
                with (output / name).open('xb') as stream: stream.write(data)
                item['icon'] = name
                break
        applications.append(item)
    dimension = picture.get('dimension', {})
    context = {'input': dimension.get('input', 'unknown'), 'dynamic_range': dimension.get('dynamicRange', 'unknown'),
               'picture_mode': dimension.get('pictureMode', 'unknown')}
    def row(identity, key, label, value, available=True, protected=False):
        return {'id': identity, 'focus_key': key, 'label': label, 'value': str(value), 'context': context,
                'protected': protected, 'available': available}
    values = picture.get('settings', {})
    settings = {'picture': [row(key, i + 1, label, values.get(key, 'Unavailable'), key in values)
                           for i, (key, label) in enumerate([('pictureMode','Picture mode'),('backlight','OLED pixel brightness'),('contrast','Contrast'),('brightness','Black level')])]}
    audio = sound.get('volumeStatus', {})
    settings['sound'] = [row(key, i + 20, label, audio.get(key, 'Unavailable'), key in audio)
                         for i, (key, label) in enumerate([('soundOutput','Sound output'),('volume','Volume'),('muteStatus','Muted')])]
    for i, (section, label) in enumerate([('network','Connection settings'),('accessibility','Accessibility settings'),('general','System settings'),('panel_care','OLED care and protections')]):
        settings[section] = [row(section, 40+i, label, 'Managed by LG', False, True)]
    captured = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')
    snapshot = {'captured_at': captured, 'applications': applications, 'launch_ids': sorted(ids),
                'inputs': [{'id': d['id'], 'label': d.get('label', d['id']), 'plugged': bool(d.get('hdmiPlugIn')),
                            'signal': bool(d.get('hdmiSignalExist')), 'active': bool(d.get('activate'))}
                           for d in inputs if re.fullmatch(r'HDMI_[1-4]', d.get('id', ''))],
                'context': context, 'settings': settings}
    (output / 'snapshot.json').write_text(json.dumps(snapshot, indent=2) + '\n')
    (output / 'read-only-receipts.json').write_text(json.dumps(receipts, indent=2) + '\n')
    print(json.dumps({'applications': len(applications), 'icons': len(list((output/'icons').iterdir())),
                      'inputs': len(snapshot['inputs']), 'captured_at': captured, 'tv_writes': False}))

if __name__ == '__main__': main()
