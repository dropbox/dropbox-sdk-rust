import os
import sys

if len(sys.argv) != 2:
    print("usage: {} <stone spec root>".format(sys.argv[0]))
    exit()

stone_root = sys.argv[1]

stone_files = []
for root, dirs, files in os.walk(stone_root):
    for filepath in files:
        if filepath.endswith(".stone"):
            stone_files.append(os.path.join(stone_root, filepath))

deps = {}
for filepath in stone_files:
    module = os.path.basename(filepath).split('.')[0]
    if module == "stone_cfg":
        continue
    deps[module] = []
    with open(filepath) as f:
        for line in f:
            if line.startswith("import "):
                deps[module].append(line.strip().split("import ")[1])

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
                for module in deps:
                    new.write('    "dbx_{}",\n'.format(module))

                in_features = False
                in_default = False
        elif in_features:
            if line.startswith("dbx_"):
                continue
            else:
                # found the end of the features list.
                # write out the new features list
                for module, imports in deps.items():
                    new.write('dbx_{} = [{}]\n'.format(
                            module,
                            ', '.join(map(lambda x: '"dbx_{}"'.format(x), imports))))
                in_features = False
        else:
            if line.startswith("dbx_"):
                in_features = True
                continue
            elif line == "default = [\n":
                in_default = True

        new.write(line)

