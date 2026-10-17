import initWasm, {
  analyze as wasmAnalyze,
  compileProject as wasmCompileProject,
  compiler_version as wasmCompilerVersion,
  decodeCartridge as wasmDecodeCartridge,
  deleteWorkcartAsset as wasmDeleteWorkcartAsset,
  exportHtml as wasmExportHtml,
  exportHtmlWorkcart as wasmExportHtmlWorkcart,
  format as wasmFormat,
  language_revision as wasmLanguageRevision,
  materializeWorkcartRecipe as wasmMaterializeWorkcartRecipe,
  packProject as wasmPackProject,
  packWorkcart as wasmPackWorkcart,
  parseProjectManifest as wasmParseProjectManifest,
  parseWorkcart as wasmParseWorkcart,
  unpackCartridge as wasmUnpackCartridge,
  unpackWorkcart as wasmUnpackWorkcart,
  compileWorkcart as wasmCompileWorkcart,
  encodeWorkcart as wasmEncodeWorkcart,
  rewriteWorkcart as wasmRewriteWorkcart,
  workcartProjectView as wasmWorkcartProjectView,
} from '../../../crates/modl-wasm/pkg/modl_wasm';

export interface SourceSpan {
  readonly file: number;
  readonly start: number;
  readonly end: number;
}

export interface CompilerDiagnostic {
  readonly code: string;
  readonly severity: 'error' | 'warning';
  readonly message: string;
  readonly primary: { readonly span: SourceSpan; readonly message: string };
  readonly secondary: readonly { readonly span: SourceSpan; readonly message: string }[];
  readonly notes: readonly string[];
}

export interface CompilerSymbol {
  readonly id: number;
  readonly name: string;
  readonly kind: string;
  readonly type: unknown;
  readonly mutable: boolean;
  readonly defined_at?: SourceSpan;
}

export interface CompilationResult {
  readonly analysis: {
    readonly tokens: readonly unknown[];
    readonly module: unknown;
    readonly symbols: readonly CompilerSymbol[];
    readonly ir: unknown;
    readonly diagnostics: readonly CompilerDiagnostic[];
  };
  readonly generated?: {
    readonly mode: 'release' | 'debug';
    readonly javascript: string;
    readonly source_map_json: string;
    readonly relationships: readonly unknown[];
    readonly generated_bytes: number;
    readonly probe_count: number;
    readonly work_model: Readonly<Record<string, number>>;
  };
}

export interface DecodedCartridge {
  readonly manifest: Readonly<Record<string, unknown>>;
  readonly entries: Readonly<Record<string, readonly number[]>>;
}

export interface ProjectManifest {
  readonly format_revision: number;
  readonly language_revision: string;
  readonly id: string;
  readonly title: string;
  readonly author: string;
  readonly version: string;
  readonly entry: string;
  readonly update_rate: 30 | 60;
  readonly hardware_revision?: 1 | 2;
  readonly compile_on_load?: boolean;
  readonly label: string | null;
  readonly thumbnail: string | null;
  readonly display: string | null;
  readonly assets: Readonly<
    Record<
      string,
      {
        readonly kind: 'sprite' | 'animation' | 'tile_set' | 'map' | 'font' | 'sound' | 'music';
        readonly path: string;
      }
    >
  >;
}

export interface UnpackedProject {
  readonly manifest: string;
  readonly files: Readonly<Record<string, readonly number[]>>;
}

export interface WorkcartProject {
  readonly source: string;
  readonly manifest: ProjectManifest;
  readonly files: Readonly<Record<string, readonly number[]>>;
  readonly recipes: readonly unknown[];
  readonly tests: readonly unknown[];
}

/** Lazy WebAssembly bridge over the repository's authoritative Rust compiler and packer. */
export class BrowserCompiler {
  private readonly initialized: Promise<void>;

  public constructor() {
    this.initialized = initWasm().then(() => undefined);
  }

  public async identity(): Promise<{ readonly compiler: string; readonly language: string }> {
    await this.initialized;
    return { compiler: wasmCompilerVersion(), language: wasmLanguageRevision() };
  }

  public async analyze(fileName: string, source: string): Promise<CompilationResult['analysis']> {
    await this.initialized;
    return JSON.parse(wasmAnalyze(fileName, source)) as CompilationResult['analysis'];
  }

  public async compileProject(
    manifest: string,
    files: Readonly<Record<string, Uint8Array>>,
    debug: boolean,
  ): Promise<CompilationResult> {
    await this.initialized;
    return JSON.parse(
      wasmCompileProject(manifest, encodeProjectFiles(files), debug),
    ) as CompilationResult;
  }

  public async parseWorkcart(source: string): Promise<WorkcartProject> {
    await this.initialized;
    return JSON.parse(wasmParseWorkcart(source)) as WorkcartProject;
  }

  public async workcartProjectView(source: string): Promise<UnpackedProject> {
    await this.initialized;
    return JSON.parse(wasmWorkcartProjectView(source)) as UnpackedProject;
  }

  public async encodeWorkcart(
    manifest: string,
    files: Readonly<Record<string, Uint8Array>>,
  ): Promise<string> {
    await this.initialized;
    return wasmEncodeWorkcart(manifest, encodeProjectFiles(files));
  }

  public async rewriteWorkcart(
    source: string,
    manifest: string,
    files: Readonly<Record<string, Uint8Array>>,
  ): Promise<string> {
    await this.initialized;
    return wasmRewriteWorkcart(source, manifest, encodeProjectFiles(files));
  }

  public async compileWorkcart(source: string, debug: boolean): Promise<CompilationResult> {
    await this.initialized;
    return JSON.parse(wasmCompileWorkcart(source, debug)) as CompilationResult;
  }

  public async materializeWorkcartRecipe(source: string, recipeId: string): Promise<string> {
    await this.initialized;
    return wasmMaterializeWorkcartRecipe(source, recipeId);
  }

  public async deleteWorkcartAsset(source: string, assetName: string): Promise<string> {
    await this.initialized;
    return wasmDeleteWorkcartAsset(source, assetName);
  }

  public async format(fileName: string, source: string): Promise<string> {
    await this.initialized;
    return wasmFormat(fileName, source);
  }

  public async packProject(
    manifest: string,
    files: Readonly<Record<string, Uint8Array>>,
  ): Promise<Uint8Array> {
    await this.initialized;
    return wasmPackProject(manifest, encodeProjectFiles(files));
  }

  public async packWorkcart(source: string): Promise<Uint8Array> {
    await this.initialized;
    return wasmPackWorkcart(source);
  }

  public async decodeCartridge(bytes: Uint8Array): Promise<DecodedCartridge> {
    await this.initialized;
    return JSON.parse(wasmDecodeCartridge(bytes)) as DecodedCartridge;
  }

  public async unpackCartridge(bytes: Uint8Array): Promise<UnpackedProject> {
    await this.initialized;
    return JSON.parse(wasmUnpackCartridge(bytes)) as UnpackedProject;
  }

  public async unpackWorkcart(bytes: Uint8Array): Promise<string> {
    await this.initialized;
    return wasmUnpackWorkcart(bytes);
  }

  public async exportHtml(
    manifest: string,
    files: Readonly<Record<string, Uint8Array>>,
  ): Promise<string> {
    await this.initialized;
    return wasmExportHtml(manifest, encodeProjectFiles(files));
  }

  public async exportHtmlWorkcart(source: string): Promise<string> {
    await this.initialized;
    return wasmExportHtmlWorkcart(source);
  }

  public async parseManifest(source: string): Promise<ProjectManifest> {
    await this.initialized;
    return JSON.parse(wasmParseProjectManifest(source)) as ProjectManifest;
  }
}

function encodeProjectFiles(files: Readonly<Record<string, Uint8Array>>): string {
  return JSON.stringify(
    Object.fromEntries(
      Object.entries(files)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([path, bytes]) => [path, [...bytes]]),
    ),
  );
}
