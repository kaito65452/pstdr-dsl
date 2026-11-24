import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const config = vscode.workspace.getConfiguration("pstdr");
  const serverPath = config.get<string>("serverPath", "pstdr-lsp");

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "pstdr" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.pstdr"),
    },
  };

  client = new LanguageClient(
    "pstdr-lsp",
    "PackStudio DSL Language Server",
    serverOptions,
    clientOptions
  );

  context.subscriptions.push(client);
  void client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
