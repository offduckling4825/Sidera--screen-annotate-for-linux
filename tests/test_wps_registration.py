#!/usr/bin/env python3
"""WPS 登记文件回归测试；使用临时 HOME，无需桌面或 WPS。"""

import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("SIDERA_BINARY", ROOT / "annotate_amd64")).resolve()
ENTRY = {
    "name": "sidera-bridge", "type": "wpp", "url": "http://127.0.0.1:16666/",
    "debug": "", "enable": "enable", "install": "null",
}


class RegistrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BINARY.is_file():
            raise RuntimeError(f"请先编译 Sidera，或用 SIDERA_BINARY 指定构建产物：{BINARY}")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="sidera-registration-")
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.path = self.home / ".local/share/Kingsoft/wps/jsaddons/publish.xml"
        self.env = dict(os.environ, HOME=str(self.home))
        # 如果安装模式误启 GUI，应立即失败，而不是依赖测试机桌面。
        self.env.pop("DISPLAY", None)
        self.env.pop("WAYLAND_DISPLAY", None)
        self.env["QT_QPA_PLATFORM"] = "sidera-no-gui-allowed"

    def seed(self, content):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_bytes(content.encode("utf-8") if isinstance(content, str) else content)

    def run_registration(self, command=None, **kwargs):
        return subprocess.run(
            command or [str(BINARY), "--register-wps-addin"],
            env=self.env, capture_output=True, timeout=15, **kwargs,
        )

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
        root = ET.parse(self.path).getroot()
        entries = [entry for entry in root.findall("jspluginonline")
                   if entry.get("name") == "sidera-bridge"]
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0].attrib, ENTRY)
        return root

    def test_create_missing_file(self):
        self.assert_success(self.run_registration())

    def test_keep_other_plugins_and_xml_content(self):
        self.seed('''<?xml version="1.0" encoding="UTF-8"?>
<?wps keep-this?>
<jsplugins version='2' xmlns:extra='urn:test'>
  <!-- keep this comment -->
  <jspluginonline name='其它插件' url='https://example.test/?a=1&amp;b=2' enable='disable'>
    <extra:settings extra:value='keep'><![CDATA[<配置>]]></extra:settings>
  </jspluginonline>
</jsplugins>''')
        root = self.assert_success(self.run_registration())
        self.assertEqual(root.attrib, {"version": "2"})
        other = root.find("jspluginonline")
        self.assertEqual(other.attrib, {
            "name": "其它插件", "url": "https://example.test/?a=1&b=2", "enable": "disable",
        })
        settings = other.find("{urn:test}settings")
        self.assertEqual(settings.attrib, {"{urn:test}value": "keep"})
        self.assertEqual(settings.text, "<配置>")
        content = self.path.read_text()
        self.assertIn("<!-- keep this comment -->", content)
        self.assertIn("<?wps keep-this?>", content)

    def test_self_closing_root_and_whitespace(self):
        for source in ("<jsplugins/>", "<jsplugins version='2' />",
                       "<jsplugins\n version='2'>\n</jsplugins >"):
            with self.subTest(source=source):
                self.seed(source)
                self.assert_success(self.run_registration())

    def test_repeat_install_does_not_rewrite(self):
        self.assert_success(self.run_registration())
        original = self.path.read_bytes()
        modified = self.path.stat().st_mtime_ns
        self.assert_success(self.run_registration())
        self.assertEqual(self.path.read_bytes(), original)
        self.assertEqual(self.path.stat().st_mtime_ns, modified)

    def test_existing_and_legacy_entries_are_unchanged(self):
        for name in ("sidera-bridge", "screen-annotate-bridge"):
            with self.subTest(name=name):
                original = f"<jsplugins><jspluginonline name='{name}' enable='disable'/></jsplugins>".encode()
                self.seed(original)
                result = self.run_registration()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.path.read_bytes(), original)

    def test_name_in_comment_url_or_nested_node_is_not_registration(self):
        self.seed('''<jsplugins>
  <!-- sidera-bridge screen-annotate-bridge </jsplugins> -->
  <jspluginonline name="other" url="https://example.test/sidera-bridge"/>
  <settings><jspluginonline name="sidera-bridge"/></settings>
</jsplugins>''')
        root = self.assert_success(self.run_registration())
        self.assertIsNotNone(root.find("settings/jspluginonline"))
        self.assertEqual(len(root.findall("jspluginonline")), 2)

    def test_utf16_file(self):
        self.seed(('<?xml version="1.0" encoding="UTF-16"?>'
                   '<jsplugins><jspluginonline name="其它插件"/></jsplugins>').encode("utf-16"))
        root = self.assert_success(self.run_registration())
        self.assertEqual(root.find("jspluginonline").get("name"), "其它插件")

    def test_invalid_files_remain_byte_for_byte_unchanged(self):
        for source in (b"", b" \n", b"<jsplugins>", b"<other/>",
                       b'<jsplugins xmlns="urn:unexpected"/>',
                       b"<jsplugins/><trailing/>",
                       b"<jsplugins><jspluginonline name='sidera-bridge'/></broken>"):
            with self.subTest(source=source):
                self.seed(source)
                result = self.run_registration()
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(result.stderr)
                self.assertEqual(self.path.read_bytes(), source)

    def test_directory_in_place_of_file_is_preserved(self):
        self.path.mkdir(parents=True)
        marker = self.path / "keep.txt"
        marker.write_text("keep")
        self.assertNotEqual(self.run_registration().returncode, 0)
        self.assertEqual(marker.read_text(), "keep")

    @unittest.skipUnless(hasattr(os, "geteuid") and os.geteuid() != 0,
                         "权限测试需要普通 Linux 用户")
    def test_unreadable_file_is_preserved(self):
        original = b"<jsplugins/>"
        self.seed(original)
        self.path.chmod(0)
        try:
            self.assertNotEqual(self.run_registration().returncode, 0)
        finally:
            self.path.chmod(0o600)
        self.assertEqual(self.path.read_bytes(), original)

    @unittest.skipUnless(os.name == "posix", "使用 Linux 文件大小限制模拟写入失败")
    def test_partial_write_does_not_truncate_original(self):
        import resource

        original = b"<jsplugins><!--" + b"x" * 8192 + b"--></jsplugins>"
        self.seed(original)

        def limit_writes():
            signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
            resource.setrlimit(resource.RLIMIT_FSIZE, (1024, 1024))

        self.assertNotEqual(self.run_registration(preexec_fn=limit_writes).returncode, 0)
        self.assertEqual(self.path.read_bytes(), original)
        self.assertEqual(list(self.path.parent.iterdir()), [self.path])

    def test_concurrent_registration_adds_only_once(self):
        self.seed("<jsplugins><jspluginonline name='other'/></jsplugins>")
        processes = []
        try:
            for _ in range(6):
                processes.append(subprocess.Popen(
                    [str(BINARY), "--register-wps-addin"], env=self.env,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                ))
            for process in processes:
                _, stderr = process.communicate(timeout=15)
                self.assertEqual(process.returncode, 0, stderr)
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                    process.communicate()
        root = self.assert_success(self.run_registration())
        self.assertEqual(len(root.findall("jspluginonline")), 2)

    def test_packaged_installer_merges_and_propagates_failure(self):
        # 模拟 /usr/bin 中的脚本：旁边没有源码构建产物，通过 PATH 找到 sidera。
        bin_dir = self.home / "bin"
        bin_dir.mkdir()
        script = bin_dir / "sidera-wps-addin-install"
        shutil.copyfile(ROOT / "wps-addin/install.sh", script)
        (bin_dir / "sidera").symlink_to(BINARY)
        self.env["PATH"] = str(bin_dir) + os.pathsep + self.env["PATH"]
        self.seed("<jsplugins><jspluginonline name='other'/></jsplugins>")
        root = self.assert_success(self.run_registration(["bash", str(script)]))
        self.assertEqual(root.find("jspluginonline").get("name"), "other")
        original = b"<broken>"
        self.seed(original)
        self.assertNotEqual(self.run_registration(["bash", str(script)]).returncode, 0)
        self.assertEqual(self.path.read_bytes(), original)


if __name__ == "__main__":
    unittest.main(verbosity=2)
