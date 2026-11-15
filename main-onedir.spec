# -*- mode: python ; coding: utf-8 -*-
# Build with tools/build_release.sh, or `pyinstaller main-onedir.spec` after `npm run build` in frontend/.
import sys

sys.path.insert(0, SPECPATH)
from pyinstaller_datas import bundle_datas

block_cipher = None

a = Analysis(['main.py'],
             pathex=[SPECPATH],
             binaries=[],
             datas=bundle_datas(SPECPATH),
             hiddenimports=['flask', 'gevent', 'engineio.async_drivers.gevent', 'psutil', 'distro', 'markdown', 'markdown.extensions.fenced_code'],
             excludes=[],
             win_no_prefer_redirects=False,
             win_private_assemblies=False,
             cipher=block_cipher,
             noarchive=False)
pyz = PYZ(a.pure, a.zipped_data,
             cipher=block_cipher)
exe = EXE(pyz,
          a.scripts,
          [],
          exclude_binaries=True,
          name='webcontrol',
          debug=False,
          strip=False,
          upx=True,
          console=True,
          # The self-updater and the Pi image expect data files beside the executable, not in _internal/.
          contents_directory='.',
          icon='webcontrolicon_256px.ico' if sys.platform == 'win32' else None)
coll = COLLECT(exe,
          a.binaries,
          a.zipfiles,
          a.datas,
          strip=False,
          upx=True,
          name='main')
