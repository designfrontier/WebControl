# -*- mode: python ; coding: utf-8 -*-
# Build with `pyinstaller main.spec` after `npm run build` in frontend/.
import sys

sys.path.insert(0, SPECPATH)
from pyinstaller_datas import bundle_datas

block_cipher = None

a = Analysis(['main.py'],
             pathex=[SPECPATH],
             binaries=[],
             datas=bundle_datas(SPECPATH),
             hiddenimports=['flask', 'gevent', 'engineio.async_drivers.gevent', 'psutil', 'distro', 'markdown', 'markdown.extensions.fenced_code'],
             hookspath=[],
             runtime_hooks=[],
             excludes=[],
             win_no_prefer_redirects=False,
             win_private_assemblies=False,
             cipher=block_cipher,
             noarchive=False)
pyz = PYZ(a.pure, a.zipped_data,
             cipher=block_cipher)
exe = EXE(pyz,
          a.scripts,
          a.binaries,
          a.zipfiles,
          a.datas,
          [],
          name='main',
          debug=False,
          bootloader_ignore_signals=False,
          strip=False,
          upx=True,
          upx_exclude=[],
          runtime_tmpdir=None,
          console=True,
          icon='webcontrolicon_256px.ico' if sys.platform == 'win32' else None)
