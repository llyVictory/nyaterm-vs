"""Version checks and registry operations for the Web release workflow."""

import argparse
import base64
import hashlib
import json
import os
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")


def version_parts(version):
    match = SEMVER.fullmatch(version)
    if not match:
        raise ValueError(f"Invalid SemVer: {version!r}")
    prerelease = match[4]
    if prerelease and any(
        part.isdigit() and len(part) > 1 and part.startswith("0")
        for part in prerelease.split(".")
    ):
        raise ValueError(f"Invalid numeric prerelease identifier: {version!r}")
    return tuple(int(match[i]) for i in (1, 2, 3)), prerelease


def release_metadata(ref, package_version, repository):
    if not ref.startswith("refs/tags/v"):
        raise ValueError("Dispatch against a version tag (refs/tags/v<SemVer>).")
    version = ref.removeprefix("refs/tags/v")
    _, prerelease = version_parts(version)
    if version != package_version:
        raise ValueError(
            f"Tag version {version!r} differs from package.json {package_version!r}."
        )
    # Docker tags cannot contain '+'. Keep build metadata in the OCI version label.
    image_tag = version.replace("+", "_")
    if len(image_tag) > 128:
        raise ValueError("Version exceeds Docker's 128-character tag limit.")
    owner = repository.split("/")[0].lower()
    return {
        "version": version,
        "image_tag": image_tag,
        "stable": str(prerelease is None).lower(),
        "image": f"ghcr.io/{owner}/nyaterm-web",
    }


def should_promote(candidate, current):
    candidate_version, candidate_prerelease = version_parts(candidate)
    if candidate_prerelease:
        return False
    if current is None:
        return True
    current_version, current_prerelease = version_parts(current)
    if current_prerelease:
        raise ValueError(
            "The existing latest image is a prerelease; refusing to replace it automatically."
        )
    return candidate_version > current_version


def verify_manifest(raw, expected_digests):
    manifest = json.loads(raw)
    actual = {
        (item["platform"]["os"], item["platform"]["architecture"]): item["digest"]
        for item in manifest["manifests"]
    }
    if len(manifest["manifests"]) != 2 or actual != expected_digests:
        raise ValueError(f"Unexpected release platforms or digests: {actual}")
    return "sha256:" + hashlib.sha256(raw).hexdigest()


class RegistryRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        redirected = super().redirect_request(request, fp, code, msg, headers, newurl)
        if (
            redirected
            and urllib.parse.urlsplit(request.full_url).netloc
            != urllib.parse.urlsplit(newurl).netloc
        ):
            redirected.remove_header("Authorization")
        return redirected


class Registry:
    def __init__(self, image, username, password):
        registry, self.repository = image.split("/", 1)
        if registry != "ghcr.io":
            raise ValueError("Only GHCR images are supported.")
        self.opener = urllib.request.build_opener(RegistryRedirect())
        credentials = base64.b64encode(f"{username}:{password}".encode()).decode()
        query = urllib.parse.urlencode(
            {"service": "ghcr.io", "scope": f"repository:{self.repository}:pull"}
        )
        response = self.get_json(
            f"https://ghcr.io/token?{query}", {"Authorization": f"Basic {credentials}"}
        )
        self.headers = {"Authorization": f"Bearer {response['token']}"}

    def get_json(self, url, headers, missing_manifest=False):
        try:
            with self.opener.open(
                urllib.request.Request(url, headers=headers), timeout=30
            ) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if missing_manifest and error.code == 404:
                payload = json.loads(error.read())
                codes = [item.get("code") for item in payload.get("errors", [])]
                if codes and all(code == "MANIFEST_UNKNOWN" for code in codes):
                    return None
            raise

    def manifest(self, reference, missing=False):
        if reference != "latest" and not DIGEST.fullmatch(reference):
            raise ValueError("Invalid manifest digest.")
        headers = self.headers | {
            "Accept": ", ".join(
                [
                    "application/vnd.oci.image.index.v1+json",
                    "application/vnd.docker.distribution.manifest.list.v2+json",
                    "application/vnd.oci.image.manifest.v1+json",
                    "application/vnd.docker.distribution.manifest.v2+json",
                ]
            )
        }
        return self.get_json(
            f"https://ghcr.io/v2/{self.repository}/manifests/{reference}",
            headers,
            missing,
        )

    def image_version(self, reference, missing=False):
        manifest = self.manifest(reference, missing)
        if manifest is None:
            return None
        if "manifests" in manifest:
            platform = next(
                item
                for item in manifest["manifests"]
                if item.get("platform", {}).get("os") == "linux"
                and item["platform"].get("architecture") == "amd64"
            )
            manifest = self.manifest(platform["digest"])
        digest = manifest["config"]["digest"]
        if not DIGEST.fullmatch(digest):
            raise ValueError("Invalid image config digest.")
        config = self.get_json(
            f"https://ghcr.io/v2/{self.repository}/blobs/{digest}", self.headers
        )
        version = config["config"]["Labels"]["org.opencontainers.image.version"]
        version_parts(version)
        return version


def promote_latest(registry, image, version, digest):
    if registry.image_version(digest) != version:
        raise ValueError(
            "Published image version does not match the candidate release."
        )
    current = registry.image_version("latest", missing=True)
    promote = should_promote(version, current)
    if promote:
        subprocess.run(
            [
                "docker",
                "buildx",
                "imagetools",
                "create",
                "--tag",
                f"{image}:latest",
                f"{image}@{digest}",
            ],
            check=True,
        )
        if registry.image_version("latest") != version:
            raise ValueError("latest verification failed.")
    return f"{'Updated' if promote else 'Kept'} latest: candidate={version}, previous={current or '(absent)'}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "command", choices=["metadata", "check-manifest", "promote-latest"]
    )
    args = parser.parse_args()
    if args.command == "metadata":
        metadata = release_metadata(
            os.environ["GITHUB_REF"],
            json.loads(Path("package.json").read_text())["version"],
            os.environ["GITHUB_REPOSITORY"],
        )
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for key, value in metadata.items():
                output.write(f"{key}={value}\n")
        print(f"Validated Web release {metadata['version']}")
    elif args.command == "check-manifest":
        digest = verify_manifest(
            Path("manifest.json").read_bytes(),
            {
                ("linux", arch): os.environ[f"DIGEST_{arch.upper()}"]
                for arch in ("amd64", "arm64")
            },
        )
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"digest={digest}\n")
    else:
        image, version, digest = (
            os.environ[key] for key in ("IMAGE", "VERSION", "DIGEST")
        )
        registry = Registry(image, os.environ["GITHUB_ACTOR"], os.environ["GH_TOKEN"])
        message = promote_latest(registry, image, version, digest)
        print(message)
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write(message + "\n")


if __name__ == "__main__":
    main()
