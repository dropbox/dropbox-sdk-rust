#!/usr/bin/env python3

import argparse
import contextlib
import os
import subprocess

from generate import generate_code
from update_manifest import update_manifest


@contextlib.contextmanager
def chdir(path: str):
    old_cwd = os.getcwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(old_cwd)


def main():
    parser = argparse.ArgumentParser(description="update the Dropbox API spec submodule")
    parser.add_argument("--spec-path", type=str, default="dropbox-api-spec",
                        help="Path to the API spec submodule.")
    parser.add_argument("--spec-rev", type=str, default=None,
                        help="git hash to update the spec to. \
                            The latest commit on the 'main' branch if unspecified.")

    args = parser.parse_args()

    generate_code(args.spec_path, gen_rust=False, gen_test=True)

    with chdir(args.spec_path):
        subprocess.run(["git", "fetch"], check=True)
        if args.spec_rev is None:
            subprocess.run(["git", "pull", "origin", "main"], check=True)
        else:
            subprocess.run(["git", "checkout", args.spec_rev], check=True)

    generate_code(args.spec_path, gen_rust=True, gen_test=False)
    update_manifest(args.spec_path)

    cargo_result = subprocess.run(["cargo", "test"])
    if cargo_result.returncode == 0:
        print()
        print("Tests from the old spec succeeded.")
        print()
        print("This means this update is likely semver-compatible.")
        print("Bump the patch version number before doing a release.")
    else:
        print()
        print("Tests from the old spec failed or failed to build.")
        print()
        print("This means the update is likely not semver-compatible.")
        print("Bump the minor version number before doing a release.")
        print()
        print("You should also run `generate.py --gen-test` to build tests for the current spec,")
        print("  and run `cargo test` again to make sure there's no problems there as well.")
        exit(1)


if __name__ == "__main__":
    main()
