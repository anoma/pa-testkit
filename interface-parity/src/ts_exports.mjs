// Prints one JSON object per exported item of a package's types entry:
// {"key": ..., "value": ...}. Usage:
// node --input-type=module --eval "$(cat ts_exports.mjs)" <package dir> <import specifier>
// The types entry is the file TypeScript resolves the specifier to from
// inside the package directory.
import { createRequire } from "node:module";
import path from "node:path";

const [pkgArg, spec] = process.argv.slice(1);
const pkgDir = path.resolve(pkgArg);
const ts = createRequire(path.join(pkgDir, "package.json"))("typescript");
const resolution = { module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler };
const resolved = ts.resolveModuleName(spec, path.join(pkgDir, "__entry__.ts"), resolution, ts.sys).resolvedModule;
if (!resolved) throw new Error(`${spec} resolves to no types entry in ${pkgDir}`);
const entryPath = resolved.resolvedFileName;
const configPath = ts.findConfigFile(pkgDir, ts.sys.fileExists, "tsconfig.json");
const options = configPath
  ? ts.parseJsonConfigFileContent(ts.readConfigFile(configPath, ts.sys.readFile).config, ts.sys, path.dirname(configPath)).options
  : {};
const program = ts.createProgram([entryPath], options);
const checker = program.getTypeChecker();
const source = program.getSourceFile(entryPath);
if (!source) throw new Error(`types entry ${entryPath} not found`);
const flags = ts.TypeFormatFlags.NoTruncation | ts.TypeFormatFlags.UseFullyQualifiedType | ts.TypeFormatFlags.InTypeAlias;
// Paths inside rendered types become relative to the package directory.
const show = (type) => checker.typeToString(type, undefined, flags).split(pkgDir).join(".");
const emit = (key, value) => console.log(JSON.stringify({ key, value }));
const typeLike = ts.SymbolFlags.Interface | ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Class | ts.SymbolFlags.Enum;

for (const exported of checker.getExportsOfModule(checker.getSymbolAtLocation(source))) {
  const symbol = exported.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(exported) : exported;
  const decl = symbol.declarations[0];
  const kind = ts.SyntaxKind[decl.kind];
  if (symbol.flags & typeLike) {
    const declared = checker.getDeclaredTypeOfSymbol(symbol);
    const alias = symbol.flags & ts.SymbolFlags.TypeAlias ? ` = ${show(declared)}` : "";
    emit(`ts ${exported.name} ${kind}`, `${kind} ${exported.name}${alias}`);
    for (const prop of checker.getPropertiesOfType(declared)) {
      const pd = prop.declarations?.[0];
      const type = pd ? checker.getTypeOfSymbolAtLocation(prop, pd) : checker.getTypeOfSymbol(prop);
      emit(`ts ${exported.name}.${prop.name} ${kind}`, `${kind} ${exported.name}.${prop.name}: ${show(type)}`);
    }
    if (symbol.flags & ts.SymbolFlags.Class) {
      emit(`ts ${exported.name} constructor`, `${kind} ${exported.name} constructor: ${show(checker.getTypeOfSymbolAtLocation(symbol, decl))}`);
    }
  } else {
    emit(`ts ${exported.name} ${kind}`, `${kind} ${exported.name}: ${show(checker.getTypeOfSymbolAtLocation(symbol, decl))}`);
  }
}
