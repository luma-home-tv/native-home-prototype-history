"""Build the separate Home client, never the Plex package or its deployment targets."""
import argparse
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('target', choices=('host', 'arm', 'test'))
parser.add_argument('--bin', choices=('control-preview', 'home-preview', 'launch-target'), default='control-preview')
args = parser.parse_args()
here = Path(__file__).resolve().parent
repo = here.parents[1]
env = os.environ.copy()
env['CARGO_INCREMENTAL'] = '0'
cargo = env.get('NATIVE_HOME_CARGO', 'cargo')
toolchain = env.get('NATIVE_HOME_RUST_TOOLCHAIN', 'nightly-2026-07-02')
common = [cargo, '+' + toolchain]
manifest = ['--locked', '--manifest-path', str(here / 'Cargo.toml')]
if args.target == 'test':
    command = common + ['test'] + manifest + ['--lib']
elif args.target == 'host':
    command = common + ['build'] + manifest + ['--features', 'hostsim', '--bin', args.bin]
else:
    sdk = Path(env['NATIVE_HOME_WEBOS_SDK']).resolve()
    output = Path(env['NATIVE_HOME_BUILD_DIR']).resolve()
    output.mkdir(parents=True, exist_ok=True)
    sysroot = sdk / 'arm-webos-linux-gnueabi/sysroot'
    cc = sdk / 'bin/arm-webos-linux-gnueabi-gcc'
    objects = []
    for source, name in [('src/svg.c', 'svg-arm.o'), ('src/compat/getauxval.c', 'getauxval-arm.o'), ('examples/native-home/sdl-events.c', 'sdl-events-arm.o')]:
        obj = output / name
        subprocess.run([str(cc), '--sysroot=' + str(sysroot), '-c', str(repo / source),
                        '-I' + str(sysroot / 'usr/include/SDL2'), '-I' + str(repo / 'src'), '-I' + str(repo / 'vendor/nanosvg'),
                        '-DNATIVE_HOME_WEBOS=1', '-fPIC', '-O2', '-o', str(obj)], check=True)
        objects.append(obj)
    env['CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABI_LINKER'] = str(cc)
    flags = ['-C target-cpu=cortex-a9', '-C target-feature=-neon',
             '-C link-arg=--sysroot=' + str(sysroot)]
    flags += ['-C link-arg=' + str(obj) for obj in objects]
    flags += ['-C link-arg=-l' + lib for lib in ('SDL2', 'SDL2_ttf', 'GLESv2', 'EGL', 'm')]
    flags += ['-C link-arg=-Wl,-rpath-link,' + str(sysroot / part) for part in ('usr/lib', 'lib')]
    env['RUSTFLAGS'] = ' '.join(flags)
    command = common + ['build'] + manifest + ['--release', '--target', 'arm-unknown-linux-gnueabi',
                 '-Z', 'build-std=std,panic_abort', '--bin', args.bin]
raise SystemExit(subprocess.run(command, cwd=repo, env=env).returncode)
