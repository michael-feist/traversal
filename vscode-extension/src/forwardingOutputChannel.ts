import * as vscode from 'vscode';

/**
 * The language client requires a LogOutputChannel and routes every server
 * stderr line through `error()`, whose typed methods prepend their own
 * `[Info]`/`[Debug]` markers. This wrapper satisfies the interface but
 * forwards all output verbatim — the server's log lines already carry
 * timestamp and level.
 */
export class ForwardingOutputChannel implements vscode.LogOutputChannel {
	private readonly channel: vscode.OutputChannel;
	private readonly onDidChangeLogLevelEmitter = new vscode.EventEmitter<vscode.LogLevel>();

	constructor(name: string) {
		this.channel = vscode.window.createOutputChannel(name);
	}

	get name(): string {
		return this.channel.name;
	}

	get logLevel(): vscode.LogLevel {
		return vscode.LogLevel.Info;
	}

	get onDidChangeLogLevel(): vscode.Event<vscode.LogLevel> {
		return this.onDidChangeLogLevelEmitter.event;
	}

	append(value: string): void {
		this.channel.append(value);
	}

	appendLine(value: string): void {
		this.channel.appendLine(value);
	}

	replace(value: string): void {
		this.channel.replace(value);
	}

	clear(): void {
		this.channel.clear();
	}

	show(preserveFocus?: boolean): void;
	show(column?: vscode.ViewColumn, preserveFocus?: boolean): void;
	show(columnOrPreserveFocus?: vscode.ViewColumn | boolean, preserveFocus?: boolean): void {
		if (typeof columnOrPreserveFocus === 'boolean' || columnOrPreserveFocus === undefined) {
			this.channel.show(columnOrPreserveFocus);
		} else {
			this.channel.show(columnOrPreserveFocus, preserveFocus);
		}
	}

	hide(): void {
		this.channel.hide();
	}

	dispose(): void {
		this.channel.dispose();
		this.onDidChangeLogLevelEmitter.dispose();
	}

	trace(message: string, ..._args: unknown[]): void {
		this.channel.appendLine(message);
	}

	debug(message: string, ..._args: unknown[]): void {
		this.channel.appendLine(message);
	}

	info(message: string, ..._args: unknown[]): void {
		this.channel.appendLine(message);
	}

	warn(message: string, ..._args: unknown[]): void {
		this.channel.appendLine(message);
	}

	error(message: string | Error, ..._args: unknown[]): void {
		this.channel.appendLine(message instanceof Error ? String(message) : message);
	}
}
