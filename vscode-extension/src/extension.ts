import * as fs from "node:fs/promises";
import { createWriteStream } from "node:fs";
import * as http from "node:http";
import * as https from "node:https";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

const REPOSITORY = "exelsior87/binary-preview";
const USER_AGENT = "binary-preview-vscode";
const LANGUAGES = ["c", "cpp", "rust", "python"];

let client: LanguageClient | undefined;

interface GitHubRelease {
  tag_name: string;
  assets: Array<{ name: string; browser_download_url: string }>;
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("binaryPreview.restartServer", async () => {
      await stopClient();
      await startClient(context);
    }),
  );

  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  try {
    const command = await resolveServer(context);
    const serverOptions: ServerOptions = { command };
    const clientOptions: LanguageClientOptions = {
      documentSelector: LANGUAGES.map((language) => ({ scheme: "file", language })),
      outputChannelName: "Binary Preview",
    };

    client = new LanguageClient(
      "binaryPreview",
      "Binary Preview",
      serverOptions,
      clientOptions,
    );
    await client.start();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(`Binary Preview: ${message}`);
  }
}

async function stopClient(): Promise<void> {
  const current = client;
  client = undefined;
  if (current) {
    await current.stop();
  }
}

async function resolveServer(context: vscode.ExtensionContext): Promise<string> {
  const configured = vscode.workspace
    .getConfiguration("binaryPreview")
    .get<string>("server.path", "")
    .trim();

  if (configured) {
    const absolute = path.resolve(configured);
    await assertFile(absolute, "Configured language server");
    return absolute;
  }

  const assetName = releaseAssetName();
  try {
    return await resolveLatestServer(context, assetName);
  } catch (error) {
    const cached = await findCachedServer(context);
    if (cached) {
      return cached;
    }
    throw error;
  }
}

async function resolveLatestServer(
  context: vscode.ExtensionContext,
  assetName: string,
): Promise<string> {
  const release = await requestJson<GitHubRelease>(
    `https://api.github.com/repos/${REPOSITORY}/releases/latest`,
  );
  const asset = release.assets.find((candidate) => candidate.name === assetName);
  if (!asset) {
    throw new Error(`release ${release.tag_name} has no asset named ${assetName}`);
  }

  const versionDirectory = vscode.Uri.joinPath(context.globalStorageUri, release.tag_name);
  const binaryName = process.platform === "win32" ? "binary-preview-lsp.exe" : "binary-preview-lsp";
  const binaryUri = vscode.Uri.joinPath(versionDirectory, binaryName);

  try {
    await assertFile(binaryUri.fsPath, "Downloaded language server");
  } catch {
    await vscode.workspace.fs.createDirectory(versionDirectory);
    const temporaryUri = vscode.Uri.joinPath(versionDirectory, `${binaryName}.download`);
    try {
      await download(asset.browser_download_url, temporaryUri.fsPath);
      await fs.rename(temporaryUri.fsPath, binaryUri.fsPath);
    } catch (error) {
      await fs.rm(temporaryUri.fsPath, { force: true }).catch(() => undefined);
      throw error;
    }
    if (process.platform !== "win32") {
      await fs.chmod(binaryUri.fsPath, 0o755);
    }
  }

  return binaryUri.fsPath;
}

async function findCachedServer(context: vscode.ExtensionContext): Promise<string | undefined> {
  const binaryName = process.platform === "win32" ? "binary-preview-lsp.exe" : "binary-preview-lsp";
  let entries;
  try {
    entries = await fs.readdir(context.globalStorageUri.fsPath, { withFileTypes: true });
  } catch {
    return undefined;
  }

  const candidates = await Promise.all(
    entries
      .filter((entry) => entry.isDirectory())
      .map(async (entry) => {
        const binaryPath = path.join(context.globalStorageUri.fsPath, entry.name, binaryName);
        try {
          const stat = await fs.stat(binaryPath);
          return stat.isFile() ? { path: binaryPath, modified: stat.mtimeMs } : undefined;
        } catch {
          return undefined;
        }
      }),
  );

  return candidates
    .filter((candidate): candidate is { path: string; modified: number } => candidate !== undefined)
    .sort((left, right) => right.modified - left.modified)[0]?.path;
}

function releaseAssetName(): string {
  const platform = process.platform;
  const architecture = process.arch;
  const names: Record<string, string> = {
    "win32-x64": "binary-preview-lsp-windows-x86_64.exe",
    "linux-x64": "binary-preview-lsp-linux-x86_64",
    "darwin-x64": "binary-preview-lsp-macos-x86_64",
    "darwin-arm64": "binary-preview-lsp-macos-aarch64",
  };
  const name = names[`${platform}-${architecture}`];
  if (!name) {
    throw new Error(`unsupported platform: ${platform}/${architecture}`);
  }
  return name;
}

async function assertFile(filePath: string, label: string): Promise<void> {
  const stat = await fs.stat(filePath);
  if (!stat.isFile()) {
    throw new Error(`${label} is not a file: ${filePath}`);
  }
}

function requestJson<T>(url: string): Promise<T> {
  return new Promise((resolve, reject) => {
    request(url, (response) => {
      const chunks: Buffer[] = [];
      response.on("data", (chunk: Buffer) => chunks.push(chunk));
      response.on("end", () => {
        try {
          resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")) as T);
        } catch (error) {
          reject(error);
        }
      });
    }, reject);
  });
}

function download(url: string, destination: string): Promise<void> {
  return new Promise((resolve, reject) => {
    request(url, (response) => {
      const stream = createWriteStream(destination);
      response.pipe(stream);
      stream.on("finish", () =>
        stream.close((error) => (error ? reject(error) : resolve())),
      );
      stream.on("error", reject);
    }, reject);
  });
}

function request(
  url: string,
  onResponse: (response: http.IncomingMessage) => void,
  onError: (error: Error) => void,
): void {
  const transport = url.startsWith("https:") ? https : http;
  transport
    .get(url, { headers: { "User-Agent": USER_AGENT, Accept: "application/vnd.github+json" } }, (response) => {
      const status = response.statusCode ?? 0;
      if (status >= 300 && status < 400 && response.headers.location) {
        response.resume();
        request(new URL(response.headers.location, url).toString(), onResponse, onError);
        return;
      }
      if (status < 200 || status >= 300) {
        response.resume();
        onError(new Error(`download request failed with HTTP ${status}`));
        return;
      }
      onResponse(response);
    })
    .on("error", onError);
}
