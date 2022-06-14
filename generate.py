#!/usr/bin/env python3

import argparse
import imp
import logging
import os
from os.path import join
from shutil import rmtree
import sys
from typing import List

sys.path.append("stone")
from stone.compiler import BackendException, Compiler
from stone.frontend.exception import InvalidSpec
from stone.frontend.frontend import specs_to_ir


class CodegenFailed(Exception):
    pass


def spec_files(spec_root: str) -> List[str]:
    specs = []
    for dirent in os.scandir(spec_root):
        if dirent.is_file() and dirent.path.endswith(".stone"):
            specs.append(dirent.path)
    return specs


def generate_code(spec_root: str, gen_rust: bool, gen_test: bool):
    """
    This is basically stone/stone/cli.py stripped down and customized to our needs.
    """

    targets = ["rust"] if gen_rust else []
    targets += ["test"] if gen_test else []

    print(f"Generating [{', '.join(targets)}] from {spec_root}")

    specs = []
    for path in spec_files(spec_root):
        with open(path) as f:
            specs.append((path, f.read()))

    try:
        api = specs_to_ir(specs)
    except InvalidSpec as e:
        print(f"{e.path}:{e.lineno}: error: {e.msg}", file=sys.stderr)
        raise CodegenFailed

    sys.path.append("generator")
    for target in targets:
        print(f"Running generator for {target}")
        try:
            backend_module = imp.load_source(
                f'{target}_backend', join("generator", f"{target}.stoneg.py"))
        except Exception:
            print(f"error: Importing backend \"{target}\" module raised an exception: ",
                  file=sys.stderr)
            raise

        destination = {
            "rust": join("src", "generated"),
            "test": join("tests", "generated"),
        }[target]

        rmtree(destination, ignore_errors=True)

        c = Compiler(api, backend_module, [], destination)
        try:
            c.build()
        except BackendException as e:
            print(f"error: {e.backend_name} raised an exception:\n{e.traceback}",
                  file=sys.stderr)
            raise CodegenFailed

        if os.linesep != "\n":
            # If this is Windows, rewrite the files to have the proper line ending.
            for dirent in os.scandir(destination):
                if dirent.is_file():
                    crlf_path = dirent.path + "_"
                    with open(dirent.path) as lf, open(crlf_path, "w") as crlf:
                        for line in lf:
                            crlf.write(line)
                    os.replace(crlf_path, dirent.path)


def main():
    parser = argparse.ArgumentParser(description="generate SDK code from the Stone API spec")
    parser.add_argument("--spec-path", type=str, default="dropbox-api-spec",
                        help="Path to the API spec submodule.")
    parser.add_argument("--gen-rust", action="store_true")
    parser.add_argument("--gen-test", action="store_true")

    args = parser.parse_args()
    if not args.gen_rust and not args.gen_test:
        args.gen_rust = True
        args.gen_test = True

    logging.basicConfig(level=logging.INFO)

    try:
        generate_code(args.spec_path, args.gen_rust, args.gen_test)
    except CodegenFailed:
        exit(2)


if __name__ == "__main__":
    main()
