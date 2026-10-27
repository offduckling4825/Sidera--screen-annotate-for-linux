#!/usr/bin/env python3
"""Run the real installer with an isolated home; requires Linux and bubblewrap."""

from pathlib import Path
import json
import shutil
import subprocess
import tempfile
import unittest
from xml.dom import minidom


ADDIN = Path(__file__).resolve().parents[1]
REGISTRATION = Path(".local/share/Kingsoft/wps/jsaddons/publish.xml")
NAMES = {"sidera-bridge", "screen-annotate-bridge"}


def project_entries(document):
    return [node for node in document.documentElement.childNodes
            if node.nodeType == node.ELEMENT_NODE
            and node.tagName == "jspluginonline"
            and node.getAttribute("name") in NAMES]


class InstallTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for command in ("bash", "bwrap", "python3"):
            if shutil.which(command) is None:
                raise RuntimeError(f"Missing test dependency: {command}")

    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="sidera-addin-install-", dir="/var/tmp")
        self.addCleanup(scratch.cleanup)
        self.scratch = Path(scratch.name)
        self.home = self.scratch / "home"
        self.home.mkdir()
        self.target = self.home / REGISTRATION
        self.source = self.scratch / "source with spaces"
        self.source.mkdir()
        self.script = self.source / "install.sh"
        # Normalize CRLF only in the isolated Linux test copy.
        self.script.write_text((ADDIN / "install.sh").read_text(encoding="utf-8"), encoding="utf-8")
        self.template = (ADDIN / "publish.xml").read_bytes()
        (self.source / "publish.xml").write_bytes(self.template)
        self.cleanup = self.source / "cleanup-old.sh"
        self.cleanup.write_text((ADDIN / "cleanup-old.sh").read_text(encoding="utf-8"), encoding="utf-8")
        # Supply the complete mount subtree. A clean host has no /usr/share/sidera
        # and bwrap cannot create that parent below the read-only system root.
        self.system_share = self.scratch / "system-share"
        self.fallback = self.system_share / "sidera/wps-addin"
        self.fallback.mkdir(parents=True)

    def existing(self, contents):
        self.target.parent.mkdir(parents=True, exist_ok=True)
        self.target.write_bytes(contents.encode("utf-8") if isinstance(contents, str) else contents)

    def run_install(self, *, readonly=False, env=None, args=()):
        command = [
            "bwrap", "--die-with-parent", "--ro-bind", "/", "/",
            "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp",
            "--bind", str(self.home), str(Path.home()),
            "--ro-bind", str(self.system_share), "/usr/share",
            "--unshare-net", "--chdir", str(self.source),
        ]
        if readonly:
            command += ["--ro-bind", str(self.target.parent), str((Path.home() / REGISTRATION).parent)]
        for key, value in (env or {}).items():
            command += ["--setenv", key, str(value)]
        return subprocess.run(command + ["/bin/bash", str(self.script), *args], text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=15)

    def assert_installed(self, result):
        self.assertEqual(result.returncode, 0, result.stdout)
        document = minidom.parseString(self.target.read_bytes())
        entries = project_entries(document)
        self.assertEqual(len(entries), 1)
        expected = project_entries(minidom.parseString(self.template))[0]
        for key, value in expected.attributes.items():
            self.assertEqual(entries[0].getAttribute(key), value)
        return document

    def backups(self):
        return sorted(self.target.parent.glob("publish.xml.sidera-backup-*"))

    def assert_preserved_failure(self, result, original):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(self.target.read_bytes(), original)
        self.assertFalse(list(self.target.parent.glob(".publish.xml-*")))

    def test_fresh_install_and_repeat_are_idempotent(self):
        self.assert_installed(self.run_install())
        original = self.target.read_bytes()
        inode = self.target.stat().st_ino
        self.assertEqual(self.target.stat().st_mode & 0o777, 0o644)
        self.assert_installed(self.run_install())
        self.assertEqual(self.target.read_bytes(), original)
        self.assertEqual(self.target.stat().st_ino, inode)
        self.assertEqual(self.backups(), [])

    def test_preserves_other_plugins_comments_attributes_and_processing_instructions(self):
        original = '''<?xml version="1.0" encoding="UTF-8"?><?wps keep="yes"?>
<jsplugins version="2" xmlns:ext="urn:other"><!-- 保留 &amp; 注释 -->
<jspluginonline name="other-addin" url="https://example.test/?a=1&amp;b=2" enable="disable"/>
<jsplugin name="local-addin"><ext:settings value="中文 &amp; 数据"/></jsplugin>
<jspluginonline name="sidera-bridge-extra" custom="keep"/></jsplugins>'''.encode()
        self.existing(original)
        before = minidom.parseString(original)
        after = self.assert_installed(self.run_install())
        self.assertEqual(dict(after.documentElement.attributes.items()),
                         dict(before.documentElement.attributes.items()))
        for node in before.documentElement.childNodes:
            self.assertIn(node.toxml(), [item.toxml() for item in after.documentElement.childNodes])
        self.assertEqual(after.childNodes[0].toxml(), before.childNodes[0].toxml())
        self.assertEqual(len(self.backups()), 1)
        self.assertEqual(self.backups()[0].read_bytes(), original)

    def test_compact_legacy_and_duplicate_entries_are_merged(self):
        self.existing('<jsplugins><jspluginonline name="other"/><jspluginonline '
                      'name="screen-annotate-bridge" url="old"/><jspluginonline '
                      'name="sidera-bridge"/><jspluginonline name="screen-annotate-bridge"/>'
                      '<jspluginonline name="screen-annotate-bridge-extra"/></jsplugins>')
        after = self.assert_installed(self.run_install())
        names = [node.getAttribute("name") for node in after.documentElement.childNodes
                 if node.nodeType == node.ELEMENT_NODE]
        self.assertEqual(names, ["other", "sidera-bridge", "screen-annotate-bridge-extra"])

    def test_existing_project_is_updated_without_changing_file_permissions(self):
        self.existing('<jsplugins><jspluginonline name="sidera-bridge" url="old" '
                      'enable="disable" custom="keep"/><jspluginonline name="other"/></jsplugins>')
        self.target.chmod(0o640)
        document = self.assert_installed(self.run_install())
        self.assertEqual(project_entries(document)[0].getAttribute("custom"), "keep")
        self.assertEqual(self.target.stat().st_mode & 0o777, 0o640)
        backups = self.backups()
        contents = self.target.read_bytes()
        inode = self.target.stat().st_ino
        self.assert_installed(self.run_install())
        self.assertEqual(self.backups(), backups)
        self.assertEqual(self.target.read_bytes(), contents)
        self.assertEqual(self.target.stat().st_ino, inode)

    def test_self_closing_root_preserves_attributes(self):
        self.existing('<jsplugins version="2"/>')
        document = self.assert_installed(self.run_install())
        self.assertEqual(document.documentElement.getAttribute("version"), "2")

    def test_invalid_existing_xml_is_rejected_unchanged(self):
        for original in (b"", b"<jsplugins><broken>", b"<other/>",
                         b'<jsplugins xmlns="urn:unknown"/>',
                         b'<!DOCTYPE jsplugins [<!ENTITY x "value">]><jsplugins/>'):
            with self.subTest(original=original):
                self.existing(original)
                self.assert_preserved_failure(self.run_install(), original)
                self.assertEqual(self.backups(), [])

    def test_cleanup_remove_current_keeps_valid_xml_and_other_plugins(self):
        home = self.scratch / "cleanup-home"
        js = home / ".local/share/Kingsoft/wps/jsaddons"
        js.mkdir(parents=True)
        publish = js / "publish.xml"
        publish.write_text('<jsplugins>\n'
                           '<jspluginonline name="other" url="https://example.test/"/>\n'
                           '<jspluginonline name="sidera-bridge" url="http://127.0.0.1:16666/"/>\n'
                           '</jsplugins>\n', encoding="utf-8")
        auth = js / "authaddin.json"
        auth.write_text(json.dumps({"wpp": {
            "current": {"enable": True, "isload": True, "name": "sidera-bridge",
                        "path": "http://127.0.0.1:16666"},
            "other": {"enable": True, "isload": True, "name": "other", "path": "https://example.test"},
            "namelist": "current;other",
        }}, indent=4), encoding="utf-8")
        result = subprocess.run(["bash", str(self.cleanup), "--home", str(home), "--remove-current"],
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=15)
        self.assertEqual(result.returncode, 0, result.stdout)
        document = minidom.parseString(publish.read_bytes())
        names = [node.getAttribute("name") for node in document.documentElement.childNodes
                 if node.nodeType == node.ELEMENT_NODE]
        self.assertEqual(names, ["other"])
        data = json.loads(auth.read_text(encoding="utf-8"))["wpp"]
        self.assertNotIn("current", data)
        self.assertEqual(data["namelist"], "other")

    def test_repair_rebuilds_a_truncated_registration_and_keeps_other_plugins(self):
        original = ('<?xml version="1.0" encoding="UTF-8"?><jsplugins>\n'
                    '<jspluginonline name="other" url="https://example.test/"/>\n'
                    '<jspluginonline name="sidera-bridge" url="old"/>\n')
        self.existing(original)
        result = self.run_install(args=["--repair"])
        document = self.assert_installed(result)
        names = [node.getAttribute("name") for node in document.documentElement.childNodes
                 if node.nodeType == node.ELEMENT_NODE]
        self.assertEqual(names, ["other", "sidera-bridge"])
        self.assertIn("登记文件损坏", result.stdout)
        self.assertEqual(len(self.backups()), 1)
        self.assertEqual(self.backups()[0].read_bytes(), original.encode())

    def test_invalid_template_is_rejected_before_modification(self):
        original = b'<jsplugins><jspluginonline name="other"/></jsplugins>'
        self.existing(original)
        for invalid in (b"broken", b"<jsplugins/>", b"<other/>"):
            with self.subTest(template=invalid):
                (self.source / "publish.xml").write_bytes(invalid)
                self.assert_preserved_failure(self.run_install(), original)
                self.assertEqual(self.backups(), [])

    def test_missing_template_is_rejected_before_creating_registration(self):
        (self.source / "publish.xml").unlink()
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse(self.target.exists())

    def test_packaged_command_uses_system_template(self):
        (self.source / "publish.xml").unlink()
        (self.fallback / "publish.xml").write_bytes(self.template)
        self.script = self.script.rename(self.source / "sidera-wps-addin-install")
        self.assert_installed(self.run_install())

    def test_readonly_directory_preserves_registration(self):
        original = b'<jsplugins><jspluginonline name="other"/></jsplugins>'
        self.existing(original)
        self.assert_preserved_failure(self.run_install(readonly=True), original)

    def test_symlink_is_rejected_without_replacing_target(self):
        self.existing(b'<jsplugins><jspluginonline name="other"/></jsplugins>')
        other = self.target.rename(self.target.parent / "other.xml")
        original = other.read_bytes()
        self.target.symlink_to("other.xml")
        self.assert_preserved_failure(self.run_install(), original)
        self.assertTrue(self.target.is_symlink())
        self.assertEqual(other.read_bytes(), original)

    def test_failed_atomic_replace_preserves_original_and_backup(self):
        original = b'<jsplugins><jspluginonline name="other"/></jsplugins>'
        self.existing(original)
        hooks = self.scratch / "failure-injection"
        hooks.mkdir()
        (hooks / "sitecustomize.py").write_text(
            "import os\ndef fail(*args, **kwargs):\n"
            "    raise OSError('injected rename failure')\nos.replace = fail\n")
        result = self.run_install(env={"PYTHONPATH": hooks})
        self.assert_preserved_failure(result, original)
        self.assertEqual(len(self.backups()), 1)
        self.assertEqual(self.backups()[0].read_bytes(), original)

    def test_install_cleans_legacy_auth_and_enables_the_current_addin(self):
        self.existing('<jsplugins><jspluginonline name="other" url="https://example.test/"/>'
                      '<jspluginonline name="screen-annotate-bridge" url="http://127.0.0.1:16666/"/>'
                      '<jspluginonline name="sidera-bridge" url="http://127.0.0.1:16666/"/></jsplugins>')
        auth = self.target.parent / "authaddin.json"
        auth.write_text(json.dumps({"wpp": {
            "old": {"enable": True, "isload": False, "name": "screen-annotate-bridge",
                    "path": "http://127.0.0.1:16666"},
            "current": {"enable": False, "isload": True, "name": "sidera-bridge",
                        "path": "http://127.0.0.1:16666"},
            "other": {"enable": True, "isload": True, "name": "other", "path": "https://example.test"},
            "namelist": "old;current;other",
        }}, indent=4), encoding="utf-8")
        self.assert_installed(self.run_install())
        data = json.loads(auth.read_text(encoding="utf-8"))["wpp"]
        self.assertNotIn("old", data)
        self.assertEqual(data["current"]["enable"], True)
        self.assertEqual(data["current"]["isload"], True)
        self.assertEqual(data["namelist"], "current;other")
        self.assertEqual(len(list(auth.parent.glob("authaddin.json.sidera-legacy-backup-*"))), 1)

    def test_missing_python_reports_error_without_modification(self):
        original = b'<jsplugins><jspluginonline name="other"/></jsplugins>'
        self.existing(original)
        commands = self.scratch / "no-python"
        commands.mkdir()
        for name in ("dirname", "readlink", "mkdir", "cp"):
            (commands / name).symlink_to(shutil.which(name))
        result = self.run_install(env={"PATH": commands})
        self.assert_preserved_failure(result, original)
        self.assertIn("python3", result.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
