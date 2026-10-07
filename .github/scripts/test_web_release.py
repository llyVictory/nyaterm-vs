import importlib.util
import io
import json
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location(
    "web_release", Path(__file__).with_name("web-release.py")
)
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class VersionTests(unittest.TestCase):
    def test_metadata_for_stable_and_prerelease(self):
        for version, stable in [("1.2.12", "true"), ("1.3.0-beta.1", "false")]:
            with self.subTest(version=version):
                metadata = release.release_metadata(
                    f"refs/tags/v{version}", version, "NyaKang/nyaterm"
                )
                self.assertEqual(metadata["version"], version)
                self.assertEqual(metadata["image_tag"], version)
                self.assertEqual(metadata["stable"], stable)
                self.assertEqual(metadata["publish"], "true")
                self.assertEqual(metadata["image"], "ghcr.io/nyakang/nyaterm-web")

    def test_build_metadata_is_preserved_in_version_and_encoded_in_tag(self):
        metadata = release.release_metadata(
            "refs/tags/v1.2.3+build.4", "1.2.3+build.4", "nyakang/nyaterm"
        )
        self.assertEqual(metadata["version"], "1.2.3+build.4")
        self.assertEqual(metadata["image_tag"], "1.2.3_build.4")

    def test_branch_dispatch_and_mismatched_version_fail(self):
        for ref in ["refs/heads/main", "refs/tags/1.2.12", "refs/tags/v1.2.11"]:
            with self.subTest(ref=ref), self.assertRaises(ValueError):
                release.release_metadata(ref, "1.2.12", "nyakang/nyaterm")

    def test_manual_main_runs_without_publishing(self):
        metadata = release.release_metadata(
            "refs/heads/main", "1.2.12", "nyakang/nyaterm", "workflow_dispatch"
        )
        self.assertEqual(metadata["version"], "1.2.12")
        self.assertEqual(metadata["publish"], "false")

    def test_manual_tag_still_publishes(self):
        metadata = release.release_metadata(
            "refs/tags/v1.2.12", "1.2.12", "nyakang/nyaterm", "workflow_dispatch"
        )
        self.assertEqual(metadata["publish"], "true")

    def test_other_branches_cannot_be_manually_released(self):
        with self.assertRaises(ValueError):
            release.release_metadata(
                "refs/heads/feature", "1.2.12", "nyakang/nyaterm", "workflow_dispatch"
            )

    def test_invalid_semver_fails(self):
        for version in [
            "1.2",
            "01.2.3",
            "1.2.3-01",
            "1.2.3-alpha.01",
            "1.2.3-",
            "1.2.3+",
            "1.2.3\n",
        ]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                release.version_parts(version)

    def test_docker_tag_length_limit(self):
        version = "1.2.3-" + "a" * 123
        with self.assertRaises(ValueError):
            release.release_metadata(
                f"refs/tags/v{version}", version, "nyakang/nyaterm"
            )

    def test_latest_creation_and_numeric_upgrade(self):
        for candidate, current in [
            ("1.2.12", None),
            ("1.2.12", "1.2.9"),
            ("1.10.0", "1.9.99"),
            ("2.0.0", "1.99.99"),
        ]:
            with self.subTest(candidate=candidate, current=current):
                self.assertTrue(release.should_promote(candidate, current))

    def test_latest_never_regresses_or_accepts_prerelease(self):
        for candidate, current in [
            ("1.2.11", "1.2.12"),
            ("1.2.12", "1.2.12"),
            ("1.2.12+rebuild", "1.2.12"),
            ("2.0.0-beta.1", "1.2.12"),
            ("2.0.0-beta.1", None),
        ]:
            with self.subTest(candidate=candidate, current=current):
                self.assertFalse(release.should_promote(candidate, current))

    def test_invalid_existing_latest_fails_closed(self):
        for current in ["unknown", "1.2.12-beta.1"]:
            with self.subTest(current=current), self.assertRaises(ValueError):
                release.should_promote("1.3.0", current)


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.digests = {
            ("linux", "amd64"): "sha256:" + "a" * 64,
            ("linux", "arm64"): "sha256:" + "b" * 64,
        }
        self.manifests = [
            {"platform": {"os": os, "architecture": arch}, "digest": digest}
            for (os, arch), digest in self.digests.items()
        ]

    def test_both_tested_platforms_required(self):
        raw = json.dumps({"manifests": self.manifests}).encode()
        self.assertRegex(
            release.verify_manifest(raw, self.digests), r"^sha256:[0-9a-f]{64}$"
        )
        for manifests in [self.manifests[:1], self.manifests + self.manifests[:1]]:
            with self.subTest(manifests=manifests), self.assertRaises(ValueError):
                release.verify_manifest(
                    json.dumps({"manifests": manifests}).encode(), self.digests
                )

    def test_untested_digest_rejected(self):
        self.manifests[0]["digest"] = "sha256:" + "c" * 64
        with self.assertRaises(ValueError):
            release.verify_manifest(
                json.dumps({"manifests": self.manifests}).encode(), self.digests
            )


class PromotionTests(unittest.TestCase):
    def setUp(self):
        self.image = "ghcr.io/nyakang/nyaterm-web"
        self.digest = "sha256:" + "a" * 64
        self.registry = Mock()

    def test_first_release_and_upgrade_publish_verified_digest(self):
        for current in [None, "1.2.9"]:
            with self.subTest(current=current), patch.object(
                release.subprocess, "run"
            ) as run:
                self.registry.image_version.side_effect = ["1.2.12", current, "1.2.12"]
                release.promote_latest(self.registry, self.image, "1.2.12", self.digest)
                run.assert_called_once_with(
                    [
                        "docker",
                        "buildx",
                        "imagetools",
                        "create",
                        "--tag",
                        f"{self.image}:latest",
                        f"{self.image}@{self.digest}",
                    ],
                    check=True,
                )

    def test_old_equal_and_prerelease_do_not_write_registry(self):
        for candidate, current in [
            ("1.2.11", "1.2.12"),
            ("1.2.12", "1.2.12"),
            ("1.3.0-beta.1", "1.2.12"),
        ]:
            with self.subTest(candidate=candidate), patch.object(
                release.subprocess, "run"
            ) as run:
                self.registry.image_version.side_effect = [candidate, current]
                release.promote_latest(
                    self.registry, self.image, candidate, self.digest
                )
                run.assert_not_called()

    def test_latest_read_failure_cannot_trigger_publication(self):
        self.registry.image_version.side_effect = [
            "1.2.12",
            urllib.error.URLError("fixture"),
        ]
        with patch.object(release.subprocess, "run") as run, self.assertRaises(
            urllib.error.URLError
        ):
            release.promote_latest(self.registry, self.image, "1.2.12", self.digest)
        run.assert_not_called()

    def test_wrong_candidate_version_cannot_trigger_publication(self):
        self.registry.image_version.return_value = "1.2.11"
        with patch.object(release.subprocess, "run") as run, self.assertRaises(
            ValueError
        ):
            release.promote_latest(self.registry, self.image, "1.2.12", self.digest)
        run.assert_not_called()

    def test_publication_error_and_failed_verification_fail_job(self):
        self.registry.image_version.side_effect = ["1.2.12", None]
        with patch.object(
            release.subprocess, "run", side_effect=RuntimeError("fixture")
        ), self.assertRaises(RuntimeError):
            release.promote_latest(self.registry, self.image, "1.2.12", self.digest)
        self.registry.image_version.side_effect = ["1.2.12", "1.2.11", "1.2.11"]
        with patch.object(release.subprocess, "run"), self.assertRaises(ValueError):
            release.promote_latest(self.registry, self.image, "1.2.12", self.digest)


class RegistryTests(unittest.TestCase):
    def setUp(self):
        self.registry = release.Registry.__new__(release.Registry)
        self.registry.opener = Mock()
        self.registry.repository = "nyakang/nyaterm-web"
        self.registry.headers = {"Authorization": "Bearer disposable-test-token"}

    def http_error(self, status, code):
        self.registry.opener.open.side_effect = urllib.error.HTTPError(
            "https://ghcr.io/v2/nyakang/nyaterm-web/manifests/latest",
            status,
            "fixture",
            {},
            io.BytesIO(json.dumps({"errors": [{"code": code}]}).encode()),
        )

    def test_only_missing_latest_can_be_treated_as_first_release(self):
        self.http_error(404, "MANIFEST_UNKNOWN")
        self.assertIsNone(self.registry.manifest("latest", missing=True))
        self.http_error(404, "MANIFEST_UNKNOWN")
        with self.assertRaises(urllib.error.HTTPError):
            self.registry.manifest("sha256:" + "a" * 64)

    def test_auth_server_and_repository_errors_fail_closed(self):
        for status, code in [
            (401, "UNAUTHORIZED"),
            (403, "DENIED"),
            (429, "TOOMANYREQUESTS"),
            (500, "UNKNOWN"),
            (404, "NAME_UNKNOWN"),
        ]:
            with self.subTest(status=status, code=code):
                self.http_error(status, code)
                with self.assertRaises(urllib.error.HTTPError):
                    self.registry.manifest("latest", missing=True)

    def test_network_failure_propagates(self):
        self.registry.opener.open.side_effect = urllib.error.URLError("fixture timeout")
        with self.assertRaises(urllib.error.URLError):
            self.registry.manifest("latest", missing=True)

    def test_version_is_read_from_amd64_config_in_index(self):
        digest = "sha256:" + "a" * 64
        self.registry.manifest = Mock(
            side_effect=[
                {
                    "manifests": [
                        {
                            "platform": {"os": "linux", "architecture": "arm64"},
                            "digest": "sha256:" + "b" * 64,
                        },
                        {
                            "platform": {"os": "linux", "architecture": "amd64"},
                            "digest": digest,
                        },
                    ]
                },
                {"config": {"digest": digest}},
            ]
        )
        self.registry.get_json = Mock(
            return_value={
                "config": {"Labels": {"org.opencontainers.image.version": "1.2.12"}}
            }
        )
        self.assertEqual(self.registry.image_version("latest"), "1.2.12")
        self.registry.manifest.assert_called_with(digest)

    def test_missing_version_label_is_an_error(self):
        self.registry.manifest = Mock(
            return_value={"config": {"digest": "sha256:" + "a" * 64}}
        )
        self.registry.get_json = Mock(return_value={"config": {"Labels": {}}})
        with self.assertRaises(KeyError):
            self.registry.image_version("latest")

    def test_registry_redirect_does_not_forward_token_to_blob_storage(self):
        request = urllib.request.Request(
            "https://ghcr.io/v2/example/blobs/fixture", headers=self.registry.headers
        )
        redirect = release.RegistryRedirect().redirect_request(
            request, None, 307, "redirect", {}, "https://storage.example/blob"
        )
        self.assertFalse(redirect.has_header("Authorization"))


if __name__ == "__main__":
    unittest.main()
