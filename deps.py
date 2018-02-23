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
            stone_files.append(os.path.join(root, filepath))

deps = {}
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

with open('deps.dot', 'w') as dot:
    dot.write('digraph deps {\n')
    for module, imports in deps.items():
        if len(imports) == 0:
            dot.write('    {};\n'.format(module))
        else:
            dot.write('    {} -> {{ {} }};\n'.format(
                module,
                ' '.join(imports)))
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
                        ', '.join(map(lambda x: '"dbx_{}"'.format(x), sorted(deps[module])))))
                in_features = False
        else:
            if line.startswith('dbx_'):
                in_features = True
                continue
            elif line == 'default = [\n':
                in_default = True

        new.write(line)

