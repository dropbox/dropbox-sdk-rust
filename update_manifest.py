#!/usr/bin/env python3

import argparse
import os
import sys
from typing import Dict, Set

from generate import spec_files

def update_manifest(stone_root: str):
    stone_files = spec_files(stone_root)

    deps: Dict[str, Set[str]] = {}
    for filepath in stone_files:
        module = None
        with open(filepath) as f:
            for line in f:
                if line.startswith('namespace '):
                    module = line.strip().split('namespace ')[1]
                    break
        if module is None or module == "":
            # this can happen if the stone file is empty
            print('unknown namespace for ' + filepath)
            continue
        print('{} = {}'.format(filepath, module))
        if module == "stone_cfg":
            continue
        if not module in deps:
            deps[module] = set()
        with open(filepath) as f:
            for line in f:
                if line.startswith('import '):
                    imported = line.strip().split('import ')[1]
                    deps[module].add(imported)

    with open('namespaces.dot', 'w') as dot:
        dot.write('digraph deps {\n')
        for module, imports in sorted(deps.items()):
            if not imports:
                dot.write('    {};\n'.format(module))
            else:
                dot.write('    {} -> {{ {} }};\n'.format(
                    module,
                    ' '.join(sorted(imports))))
        dot.write('}\n')

    # super hacky toml reader and editor
    in_features = False
    in_default = False
    with open("Cargo.toml", "r") as old, open("Cargo.toml.new", "w") as new:
        for line in old:
            if in_default:
                if line.startswith("    \"dbx_"):
                    in_features = True
                    continue
                else:
                    # found the end
                    # write an indented list of features
                    for module in sorted(deps):
                        new.write('    "dbx_{}",\n'.format(module))

                    in_features = False
                    in_default = False
            elif in_features:
                if line.startswith("dbx_"):
                    continue
                else:
                    # found the end of the features list.
                    # write out the new features list
                    for module in sorted(deps):
                        new.write('dbx_{} = [{}]\n'.format(
                            module,
                            ', '.join(['"dbx_{}"'.format(x) for x in sorted(deps[module])])))
                    in_features = False
            else:
                if line.startswith('dbx_'):
                    in_features = True
                    continue
                elif line == 'default = [\n':
                    in_default = True

            new.write(line)

    os.replace("Cargo.toml.new", "Cargo.toml")


def main():
    parser = argparse.ArgumentParser(description="update the namespace features in Cargo.toml")
    parser.add_argument("--spec-path", type=str, default="dropbox-api-spec",
                        help="Path to the API spec submodule.")

    args = parser.parse_args()
    update_manifest(args.spec_path)


if __name__ == "__main__":
    main()
