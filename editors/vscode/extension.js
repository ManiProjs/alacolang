const vscode = require('vscode');
const { spawn } = require('child_process');

function activate(context) {
  const output = vscode.window.createOutputChannel('Alaco');
  context.subscriptions.push(output);
  const run = (action) => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'alaco') {
      vscode.window.showWarningMessage('Open an Alaco (.aco) file first.');
      return;
    }
    const file = editor.document.uri.fsPath;
    const executable = vscode.workspace.getConfiguration('alaco').get('executable', 'alaco');
    const cwd = vscode.workspace.getWorkspaceFolder(editor.document.uri)?.uri.fsPath;
    output.appendLine(`$ ${executable} ${action} ${file}`);
    output.show(true);
    const child = spawn(executable, [action, file], { cwd, windowsHide: true });
    child.stdout.on('data', (chunk) => output.append(chunk.toString()));
    child.stderr.on('data', (chunk) => output.append(chunk.toString()));
    child.on('error', (error) => {
      output.appendLine(`Failed to start Alaco: ${error.message}`);
      vscode.window.showErrorMessage(`Could not start Alaco: ${error.message}. Check alaco.executable and PATH.`);
    });
    child.on('close', (code) => {
      output.appendLine(`Alaco ${action} exited with code ${code}`);
      if (code === 0) vscode.window.setStatusBarMessage(`Alaco: ${action} succeeded`, 3000);
      else vscode.window.showErrorMessage(`Alaco ${action} failed (exit code ${code ?? 'unknown'}). See the Alaco output channel.`);
    });
  };

  for (const action of ['check', 'build', 'run']) {
    context.subscriptions.push(vscode.commands.registerCommand(`alaco.${action}`, () => run(action)));
  }
}

function deactivate() {}
module.exports = { activate, deactivate };
