#!/usr/bin/env python3
"""Run inside tools/run_isolated.py; never starts a real Sidera/WPS or desktop."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock
from usb_autostart import MANAGED, Autostart, desktop_fields

spec = importlib.util.spec_from_file_location("usb", Path(__file__).with_name("sidera_usb.py"))
usb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(usb)


class KitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="usb-test-")
        self.root = Path(self.temp.name)
        self.home = self.root / "用户 with spaces"
        self.home.mkdir()
        self.bundle = self.root / "U 盘"
        self.bundle.mkdir()
        self.original = self.root / "system/usr/bin/sidera"
        self.original.parent.mkdir(parents=True)
        shutil.copy2("/bin/true", self.original)
        self.old_config = self.home / ".config/sidera/config"
        self.old_config.parent.mkdir(parents=True)
        self.old_config.write_text("theme=light\nold-user-setting=keep\n")
        self.wps_path = self.home / ".local/share/Kingsoft/wps/jsaddons/publish.xml"
        self.wps_path.parent.mkdir(parents=True)
        self.wps_path.write_bytes(b'<jsplugins><jspluginonline name="screen-annotate-bridge" url="old"/><jspluginonline name="other" url="keep"/></jsplugins>')
        self.before = {str(p): usb.digest(p) for p in (self.original, self.old_config, self.wps_path)}
        self.patch_binary = mock.patch.object(usb, "ORIGINAL_BINARY", self.original)
        self.patch_binary.start()
        original_capture = usb.capture
        def capture(args, **kwargs):
            if args[:3] == ["dpkg-query", "-L", "sidera"]:
                return str(self.original) + "\n"
            return original_capture(args, **kwargs)
        self.patch_capture = mock.patch.object(usb, "capture", side_effect=capture)
        self.patch_capture.start()
        self.manager = usb.Manager(self.bundle, self.home)
        self.arch = subprocess.check_output(["dpkg", "--print-architecture"], text=True).strip()
        self.manager.system = {"architecture": self.arch, "glibc": os.confstr("CS_GNU_LIBC_VERSION").split()[-1], "free_mb": 1000}
        self.manager.examine = lambda: None

    def tearDown(self):
        self.manager.lock.close()
        self.patch_capture.stop()
        self.patch_binary.stop()
        self.temp.cleanup()

    def assert_original_unchanged(self):
        self.assertEqual({p: usb.digest(p) for p in self.before}, self.before)

    def package(self, name="good", binary="/bin/true", version="3.0-Electro-testing+custom20261025.34"):
        path = self.bundle / (name + ".tar.gz")
        with tarfile.open(path, "w:gz", dereference=True) as archive:
            archive.add(binary, arcname="bin/sidera", recursive=False)
            for filename, data in (("icon.png", b"icon"), ("share/wps-addin/publish.xml", b'<jsplugins><jspluginonline name="sidera-bridge" url="new" enable_dev="enable"/></jsplugins>')):
                member = tarfile.TarInfo(filename)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        return {"file": str(path), "sha256": usb.digest(path), "version": version,
                "kind": "portable", "architecture": self.arch, "variant": name}

    def install(self, entries=None):
        self.manager.candidates = lambda: entries or [self.package()]
        self.manager.install()
        return usb.load_json(self.manager.base / "state.json")

    def test_install_keeps_original_and_configs_and_creates_separate_entries(self):
        state = self.install()
        self.assertEqual(state["mode"], "new")
        self.assert_original_unchanged()
        for suffix in ("new", "original", "manager"):
            self.assertTrue((self.home / (".local/share/applications/sidera-usb-" + suffix + ".desktop")).is_file())
        self.assertFalse((self.home / ".config/autostart/sidera.desktop").exists())
        self.assertTrue((self.manager.base / "backup-original/user-files/.config/sidera/config").exists())

    def test_corrupt_first_candidate_falls_back_to_verified_payload(self):
        bad = self.package("bad")
        bad["sha256"] = "0" * 64
        state = self.install([bad, self.package("backup")])
        self.assertEqual(state["variant"], "backup")
        self.assert_original_unchanged()

    def test_all_candidates_fail_preserves_previous_selection(self):
        previous = self.install()
        bad = self.package("bad")
        bad["sha256"] = "0" * 64
        self.install([bad])
        self.assertEqual(usb.load_json(self.manager.base / "state.json"), previous)
        self.assert_original_unchanged()

    def test_offline_does_not_contact_network(self):
        entry = self.package()
        usb.save_json(self.bundle / "catalog.json", {"packages": [dict(entry, file=Path(entry["file"]).name)]})
        with mock.patch("socket.create_connection", side_effect=AssertionError("network")):
            self.assertEqual(len(self.manager.candidates()), 1)

    def test_offline_catalog_selects_newest_version_first(self):
        older = self.package("older")
        newer = self.package("newer", version="3.0-Electro-testing+custom20261025.35")
        usb.save_json(self.bundle / "catalog.json", {"packages": [dict(entry, file=Path(entry["file"]).name) for entry in (older, newer)]})
        with mock.patch("socket.create_connection", side_effect=AssertionError("network")):
            self.assertEqual(self.manager.candidates()[0]["sha256"], newer["sha256"])

    def test_selection_commit_is_rolled_back_if_shortcut_write_fails(self):
        previous = self.install()
        with mock.patch.object(self.manager, "shortcuts", side_effect=OSError("disk full")):
            self.install([self.package("next", version="3.0-Electro-testing+custom20261025.35")])
        self.assertEqual(usb.load_json(self.manager.base / "state.json"), previous)
        self.assert_original_unchanged()

    def test_changed_installed_file_is_detected(self):
        state = self.install()
        path = self.manager.base / "versions" / state["current"]
        (path / "icon.png").write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "损坏"):
            self.manager.verify(path)

    def test_new_process_gets_separate_home_and_xauth(self):
        state = self.install()
        (self.home / ".Xauthority").write_text("fixture-only")
        with mock.patch.dict(os.environ, {"DISPLAY": ":999", "XDG_SESSION_TYPE": "x11"}, clear=True), mock.patch.object(self.manager, "run", return_value=0) as launch:
            self.manager.launch()
        env = launch.call_args.args[1]
        self.assertNotEqual(env["HOME"], str(self.home))
        self.assertIn(state["current"], env["HOME"])
        self.assertEqual(env["XAUTHORITY"], str(self.home / ".Xauthority"))
        self.assert_original_unchanged()

    def test_failed_program_automatically_falls_back_without_killing_anyone(self):
        self.install([self.package("failure", "/bin/false")])
        with mock.patch.dict(os.environ, {"DISPLAY": ":999", "XDG_SESSION_TYPE": "x11"}, clear=True):
            self.manager.launch()  # Only /bin/false and the /bin/true original fixture.
        self.assertEqual(usb.load_json(self.manager.base / "state.json")["mode"], "original")
        self.assertEqual(self.manager.result, "原版正常退出")
        self.assert_original_unchanged()

    def test_old_abi_and_wrong_arch_are_rejected_before_execution(self):
        with self.assertRaisesRegex(ValueError, "glibc"):
            usb.inspect_elf("/bin/true", self.arch, "2.1")
        with self.assertRaisesRegex(ValueError, "架构"):
            usb.inspect_elf("/bin/true", "arm64" if self.arch == "amd64" else "amd64", "99.0")

    def test_archive_paths_and_links_are_rejected(self):
        for name, kind in (("../escape", tarfile.REGTYPE), ("/escape", tarfile.REGTYPE), ("share/link", tarfile.SYMTYPE)):
            data = io.BytesIO()
            with tarfile.open(fileobj=data, mode="w") as archive:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.linkname = "/etc/passwd"
                archive.addfile(member)
            data.seek(0)
            with self.assertRaises(ValueError):
                usb.unpack(data, self.root / "extract")
        self.assertFalse((self.root / "escape").exists())

    def test_deb_scripts_are_never_extracted_or_executed(self):
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w") as archive:
            for name, content in (("DEBIAN/postinst", b"touch /tmp/unsafe"), ("usr/bin/sidera", Path("/bin/true").read_bytes())):
                member = tarfile.TarInfo(name)
                member.size = len(content)
                archive.addfile(member, io.BytesIO(content))
        data.seek(0)
        target = self.root / "unpack"
        usb.unpack(data, target, deb=True)
        self.assertFalse((target / "DEBIAN").exists())
        self.assertEqual(list(target.rglob("sidera")), [target / "bin/sidera"])

    def test_wps_registration_and_restore_keep_other_addins(self):
        self.install()
        self.manager.wps()
        current = self.wps_path.read_bytes().replace(b'</jsplugins>', b'<jspluginonline name="later" url="keep-too"/></jsplugins>')
        self.wps_path.write_bytes(current)
        self.manager.restore_wps()
        text = self.wps_path.read_text()
        self.assertIn('name="screen-annotate-bridge"', text)
        self.assertIn('name="other"', text)
        self.assertIn('name="later"', text)
        self.assertNotIn('name="sidera-bridge"', text)

    def test_wps_journal_recovers_interruption_after_mutation(self):
        self.install()
        real_atomic = usb.atomic
        def interrupted(path, content):
            real_atomic(path, content)
            if Path(path) == self.wps_path:
                raise OSError("power loss after replacement")
        with mock.patch.object(usb, "atomic", side_effect=interrupted):
            with self.assertRaises(OSError):
                self.manager.wps()
        self.manager.restore_wps()
        self.assert_original_unchanged()

    def test_wps_invalid_xml_is_not_overwritten(self):
        self.install()
        self.wps_path.write_bytes(b"<damaged")
        with self.assertRaises(Exception):
            self.manager.wps()
        self.assertEqual(self.wps_path.read_bytes(), b"<damaged")

    def test_original_restore_only_changes_manager_selection(self):
        self.install()
        self.manager.restore()
        self.assertEqual(usb.load_json(self.manager.base / "state.json")["mode"], "original")
        self.assert_original_unchanged()

    def test_concurrent_manager_refused(self):
        with self.assertRaisesRegex(RuntimeError, "正在运行"):
            usb.Manager(self.bundle, self.home)

    def test_report_is_mirrored_and_older_offline_report_is_exported(self):
        (self.bundle / "kit-id.txt").write_text(usb.KIT_ID)
        usb.save_json(self.manager.base / "usb-location.json", {"path": str(self.bundle), "id": usb.KIT_ID})
        older = self.manager.base / "logs/older"
        older.mkdir()
        (older / "report.txt").write_text("offline")
        self.manager.result = "test"
        self.manager.finish()
        self.assertTrue((self.bundle / "logs" / self.manager.report.name / "report.html").is_file())
        self.assertEqual((self.bundle / "logs/older/report.txt").read_text(), "offline")

    def test_catalog_path_escape_rejected(self):
        entry = self.package()
        entry["file"] = "../outside.tar.gz"
        usb.save_json(self.bundle / "catalog.json", {"packages": [entry]})
        with self.assertRaisesRegex(ValueError, "越界"):
            self.manager.candidates()

    def test_partial_backup_is_retained_and_recreated_before_install(self):
        (self.manager.base / "backup-original").mkdir()
        (self.manager.base / "backup-original/partial").write_text("keep")
        self.install()
        retained = list(self.manager.base.glob("backup-incomplete-*/partial"))
        self.assertEqual(len(retained), 1)
        self.assertEqual(retained[0].read_text(), "keep")
        self.assertTrue(self.manager.original["available"])
        self.assert_original_unchanged()

    def autostart_fixture(self):
        self.install()
        controller = self.manager.autostart
        controller.system_dirs = [self.root / "system-autostart"]
        controller.directory.mkdir(parents=True, exist_ok=True)
        controller.system_dirs[0].mkdir(exist_ok=True)
        local = controller.directory / "sidera.desktop"
        local.write_bytes(b'[Desktop Entry]\nType=Application\nName=Sidera original\nExec=/usr/bin/sidera\nX-GNOME-Autostart-enabled=true\n')
        local.chmod(0o640)
        system = controller.system_dirs[0] / "sidera-vendor.desktop"
        system.write_bytes(b'[Desktop Entry]\nType=Application\nName=Sidera\nExec=sidera\n')
        unrelated = controller.directory / "unrelated.desktop"
        unrelated.write_bytes(b'[Desktop Entry]\nType=Application\nExec=some-other-app\n')
        return controller, {str(p): p.read_bytes() for p in (local, system, unrelated)}

    def effective_entries(self, controller):
        entries = {}
        for directory in list(reversed(controller.system_dirs)) + [controller.directory]:
            for path in directory.glob('sidera*.desktop'):
                entries[path.name] = desktop_fields(path.read_bytes())
        return [name for name, fields in entries.items() if fields.get('hidden') != 'true']

    def test_default_install_keeps_legacy_autostart_without_enabling_new(self):
        path = self.home / '.config/autostart/sidera.desktop'
        path.parent.mkdir(parents=True)
        path.write_bytes(b'[Desktop Entry]\nType=Application\nExec=sidera\n')
        before = path.read_bytes()
        state = self.install()
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((path.parent / MANAGED).exists())
        flag = self.manager.base / 'profiles' / state['current'] / '.config/autostart/sidera.desktop'
        self.assertFalse(flag.exists())

    def test_autostart_takes_over_multiple_entries_and_restores_exact_bytes(self):
        controller, before = self.autostart_fixture()
        controller.enable()
        self.assertEqual(self.effective_entries(controller), [MANAGED])
        self.assertTrue(controller.enabled())
        controller.restore()
        self.assertEqual({p: Path(p).read_bytes() for p in before}, before)
        self.assertEqual((controller.directory / 'sidera.desktop').stat().st_mode & 0o777, 0o640)
        self.assertFalse((controller.directory / MANAGED).exists())

    def test_autostart_off_keeps_legacy_masked_and_reenable_is_idempotent(self):
        controller, before = self.autostart_fixture()
        controller.enable()
        original_records = controller.state()['entries']
        controller.set_enabled(False)
        self.assertEqual(self.effective_entries(controller), [])
        controller.enable()
        controller.enable()
        self.assertEqual(self.effective_entries(controller), [MANAGED])
        self.assertEqual(controller.state()['entries'], original_records)
        controller.restore()
        self.assertEqual({p: Path(p).read_bytes() for p in before}, before)

    def test_no_legacy_autostart_is_not_invented_on_restore(self):
        self.install()
        controller = self.manager.autostart
        controller.system_dirs = []
        controller.enable()
        controller.restore()
        self.assertEqual(list(controller.directory.glob('*.desktop')), [])

    def test_interrupted_autostart_switch_recovers_on_next_manager(self):
        controller, before = self.autostart_fixture()
        real_write = controller.write
        class PowerLoss(BaseException):
            pass
        def power_loss(path, data):
            real_write(path, data)
            if Path(path).name == 'sidera.desktop':
                raise PowerLoss()
        controller.write = power_loss
        with self.assertRaises(PowerLoss):
            controller.enable()
        self.assertEqual(controller.state()['phase'], 'preparing')
        controller.write = real_write
        controller.recover()
        self.assertTrue(controller.recovered)
        self.assertEqual({p: Path(p).read_bytes() for p in before}, before)
        self.assertFalse((controller.directory / MANAGED).exists())

    def test_autostart_write_failure_rolls_back_without_erasing_backups(self):
        controller, before = self.autostart_fixture()
        real_write = controller.write
        failed = False
        def fail_once(path, data):
            nonlocal failed
            if Path(path).name == 'sidera-vendor.desktop' and not failed:
                failed = True
                raise OSError('disk full')
            real_write(path, data)
        controller.write = fail_once
        with self.assertRaises(OSError):
            controller.enable()
        self.assertEqual({p: Path(p).read_bytes() for p in before}, before)
        self.assertEqual(controller.state()['phase'], 'restored')

    def test_autostart_external_changes_are_not_overwritten(self):
        controller, _ = self.autostart_fixture()
        controller.enable()
        changed = controller.directory / 'sidera.desktop'
        changed.write_bytes(b'[Desktop Entry]\nExec=changed-by-user\n')
        with self.assertRaisesRegex(RuntimeError, '外部修改'):
            controller.restore()
        self.assertIn(b'changed-by-user', changed.read_bytes())

    def test_private_app_toggle_controls_the_actual_single_autostart_entry(self):
        controller, _ = self.autostart_fixture()
        state = usb.load_json(self.manager.base / 'state.json')
        profile = self.manager.prepare_profile(self.manager.base / 'versions' / state['current'])
        self.manager.profile_autostart_flag(profile, True)
        previous = self.manager.sync_autostart_toggle(profile, False)
        self.assertTrue(previous)
        self.assertEqual(self.effective_entries(controller), [MANAGED])
        self.manager.profile_autostart_flag(profile, False)
        self.assertFalse(self.manager.sync_autostart_toggle(profile, previous))
        self.assertEqual(self.effective_entries(controller), [])

    def test_crash_fallback_restores_original_autostart(self):
        controller, before = self.autostart_fixture()
        self.install([self.package('crashing', '/bin/false')])
        controller.enable()
        with mock.patch.dict(os.environ, {'DISPLAY': ':999', 'XDG_SESSION_TYPE': 'x11'}, clear=True):
            self.manager.launch()
        self.assertEqual(self.manager.result, '原版正常退出')
        self.assertEqual({p: Path(p).read_bytes() for p in before}, before)
        self.assertFalse(controller.enabled())

    def test_running_child_switch_is_observed_without_real_desktop(self):
        controller, _ = self.autostart_fixture()
        state = usb.load_json(self.manager.base / 'state.json')
        profile = self.manager.prepare_profile(self.manager.base / 'versions' / state['current'])
        child = self.root / 'toggle-fixture'
        child.write_text('#!/usr/bin/python3\nfrom pathlib import Path\nimport os,time\n'
                         'p=Path(os.environ["HOME"])/".config/autostart/sidera.desktop"\n'
                         'p.parent.mkdir(parents=True,exist_ok=True)\np.write_text("enabled")\n'
                         'time.sleep(1.1)\np.unlink()\ntime.sleep(1.1)\n')
        child.chmod(0o700)
        code = self.manager.run(child, {'HOME': str(profile), 'PATH': '/usr/bin:/bin'}, 'fixture', profile)
        self.assertEqual(code, 0)
        self.assertEqual(controller.state()['phase'], 'active')
        self.assertFalse(controller.enabled())
        self.assertEqual(self.effective_entries(controller), [])

    def test_autostart_refuses_a_linked_user_entry(self):
        self.install()
        controller = self.manager.autostart
        controller.system_dirs = []
        controller.directory.mkdir(parents=True)
        outside = self.root / 'unrelated-config'
        outside.write_text('[Desktop Entry]\nExec=sidera\n')
        (controller.directory / 'sidera.desktop').symlink_to(outside)
        before = outside.read_bytes()
        with self.assertRaisesRegex(RuntimeError, '符号链接'):
            controller.enable()
        self.assertEqual(outside.read_bytes(), before)
        self.assertFalse((controller.directory / MANAGED).exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
