import CodeEditor from "@/components/runbooks/editor/blocks/Editor/Editor";
import {
  BlockNoteEditor,
  BlockNoteSchema,
  defaultBlockSpecs,
  defaultInlineContentSpecs,
} from "@blocknote/core";
import { en } from "@blocknote/core/locales";
import Directory from "./blocks/Directory";
import Env from "./blocks/Env";
import Var from "./blocks/Var";
import VarDisplay from "./blocks/VarDisplay";
import MarkdownRender from "./blocks/MarkdownRender";
import LocalVar from "./blocks/LocalVar";
import Prometheus from "./blocks/Prometheus/Prometheus";
import SQLite from "./blocks/SQLite/SQLite";
import Postgres from "./blocks/Postgres/Postgres";
import MySQL from "./blocks/MySQL/MySQL";
import Clickhouse from "./blocks/Clickhouse/Clickhouse";
import { HttpBlockSpec } from "@/lib/blocks/http";
import { LocalDirectoryBlockSpec } from "@/lib/blocks/localdirectory";
import Script from "./blocks/Script/Script";
import SshConnect from "./blocks/ssh/SshConnect";
import HostSelect from "./blocks/Host";

import { randomColor } from "@/lib/colors";
import PhoenixProvider from "@/lib/phoenix_provider";
import { User } from "@/state/models";
import * as Y from "yjs";
import * as awarenessProtocol from "y-protocols/awareness";
import Dropdown from "./blocks/Dropdown/Dropdown";
import DevConsole from "@/lib/dev/dev_console";
import { TerminalBlockSpec } from "@/lib/blocks/terminal";
import { KubernetesBlockSpec } from "@/lib/blocks/kubernetes";
import { RunbookLink } from "./inline/RunbookLink";
import HorizontalRule from "./blocks/HorizontalRule";
import { withoutProperties } from "@/lib/utils";

// Our schema with block specs, which contain the configs and implementations for blocks
// that we want our editor to use.
export const schema = BlockNoteSchema.create({
  blockSpecs: {
    ...withoutProperties(defaultBlockSpecs, ["divider"]),

    // Execution
    run: TerminalBlockSpec(),
    "kubernetes-get": KubernetesBlockSpec(),
    script: Script(),
    directory: Directory(),
    "local-directory": LocalDirectoryBlockSpec(),
    env: Env(),
    var: Var(),
    var_display: VarDisplay(),
    markdown_render: MarkdownRender(),
    "local-var": LocalVar(),
    dropdown: Dropdown(),

    // Monitoring
    prometheus: Prometheus(),

    // Databases
    sqlite: SQLite(),
    postgres: Postgres(),
    mysql: MySQL(),
    clickhouse: Clickhouse(),

    // Network
    http: HttpBlockSpec(),
    "ssh-connect": SshConnect(),
    "host-select": HostSelect(),

    // Misc
    editor: CodeEditor(),
    horizontal_rule: HorizontalRule(),
  },
  inlineContentSpecs: {
    // Adds all default inline content.
    ...defaultInlineContentSpecs,
    // Adds runbook link inline content.
    "runbook-link": RunbookLink,
  },
});

export function createBasicEditor(content: any) {
  let editor = BlockNoteEditor.create({
    schema,
    initialContent: content,
  });

  DevConsole.addAppObject("editor", editor);
  return editor;
}

export function createLocalOnlyEditor(content: any) {
  let editor = BlockNoteEditor.create({
    schema,
    _tiptapOptions: {
      editorProps: {
        scrollThreshold: 200,
        scrollMargin: 200,
      },
    },
    dictionary: {
      ...en,
    },
    initialContent: content,
  });

  DevConsole.addAppObject("editor", editor);
  return editor;
}

export function createCollaborativeEditor(
  provider: PhoenixProvider,
  user: User,
  presenceColor?: string,
) {
  presenceColor = presenceColor || randomColor();
  let editor = BlockNoteEditor.create({
    schema,
    _tiptapOptions: {
      editorProps: {
        scrollThreshold: 200,
        scrollMargin: 200,
      },
    },
    collaboration: {
      provider: provider,
      fragment: provider.doc.getXmlFragment("document-store"),
      user: {
        name: user.username || "Anonymous",
        color: presenceColor,
      },
    },
    dictionary: {
      ...en,
    },
  });

  DevConsole.addAppObject("editor", editor);
  return editor;
}

export function createConversionEditor(doc: Y.Doc, fragment: Y.XmlFragment) {
  return BlockNoteEditor.create({
    schema,
    collaboration: {
      provider: new NullProvider(doc),
      fragment: fragment,
      user: {
        name: "Conversion",
        color: randomColor(),
      },
    },
  });
}

class NullProvider {
  public readonly awareness: awarenessProtocol.Awareness;

  constructor(doc: Y.Doc) {
    this.awareness = new awarenessProtocol.Awareness(doc);
  }
}
