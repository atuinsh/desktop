import { Tree, TreeData, Node, DeleteStrategy, TraversalOrder } from "@/lib/tree";
import { Some, Option } from "@binarymuse/ts-stdlib";
import { uuidv4 } from "uuidv7";

export type Folder = TreeData<FolderItem>;

export type ArboristNode = {
  type: "folder" | "runbook";
  id: string;
  name?: string;
  children?: ArboristTree;
};

export type ArboristTree = Array<ArboristNode>;

export type FolderItem =
  | {
      type: "folder";
      id: string;
      name: string;
    }
  | {
      type: "runbook";
      id: string;
    };

export default class WorkspaceFolder {
  private root: Node<FolderItem>;

  static fromJS(data: Folder): WorkspaceFolder {
    return new WorkspaceFolder(data);
  }

  private constructor(data: Folder) {
    this.root = Tree.fromJS<FolderItem>(data);
  }

  // Note: its important that this method returns an object
  // with as many shared references to the underlying data as possible;
  // by default, `jsondiffpatch` uses reference equality to determine
  // if two objects are the same, so we need to ensure that the same
  // object is returned for the same id unless it's actually changed.
  public toJS(): Folder {
    return this.root.toJS();
  }

  public toArborist(): ArboristTree {
    const data: ArboristTree = [];
    const root = this.root;

    function extractName(data: FolderItem): string | undefined {
      if (data.type === "folder") {
        return data.name;
      }

      return undefined;
    }

    function processNode(node: Node<FolderItem>, container: ArboristTree): void {
      const children = node.children();
      for (const child of children) {
        const childrenContainer: ArboristTree = [];
        if (child.getData().unwrap().type === "folder") {
          processNode(child, childrenContainer);
        }

        const data: ArboristNode = {
          type: child.getData().unwrap().type,
          id: child.id() as string,
          name: child.id() as string,
        };

        if (child.getData().unwrap().type === "folder") {
          data.name = child.getData().map(extractName).unwrapOr(undefined);
          data.children = childrenContainer;
        }

        container.push(data);
      }
    }

    processNode(root, data);

    return data;
  }

  public getNode(id: string): Option<Node<FolderItem>> {
    return this.root.getNode(id);
  }

  public getRunbooks(): string[] {
    return this.root
      .descendants(TraversalOrder.BreadthFirst)
      .filter((n) => n.getData().unwrap().type === "runbook")
      .map((n) => n.id() as string);
  }

  public createFolder(id: string, name: string, parentId: string | null): boolean {
    const parent = parentId ? this.root.getNode(parentId) : Some(this.root.root());

    return parent
      .map((p) =>
        p.createChild(id, 0).setData({
          type: "folder",
          id,
          name,
        }),
      )
      .map(() => true)
      .unwrapOr(false);
  }

  public createRunbook(id: string, parentId: string | null): Node<FolderItem> {
    const parent = parentId
      ? this.root.getNode(parentId).expect(`Couldn't find parent with id ${parentId}`)
      : this.root.root();

    return parent.createChild(id, 0).setData({
      type: "runbook",
      id,
    });
  }

  public renameFolder(id: string, newName: string): boolean {
    const node = this.root.getNode(id);
    const data = node.andThen((n) => n.getData());

    if (data.isSome()) {
      const item = data.unwrap();
      if (item.type === "folder") {
        item.name = newName;
        return true;
      }

      return false;
    }

    return false;
  }

  public deleteFolder(id: string): boolean {
    const node = this.root.getNode(id);
    const data = node.andThen((n) => n.getData());

    if (data.isSome() && data.unwrap().type === "folder") {
      node.unwrap().delete(DeleteStrategy.Cascade);
      return true;
    }

    return false;
  }

  public deleteRunbook(id: string): boolean {
    const node = this.root.getNode(id);
    const data = node.andThen((n) => n.getData());

    if (data.isSome() && data.unwrap().type === "runbook") {
      node.unwrap().delete(DeleteStrategy.Decline);
      return true;
    }

    return false;
  }

  public moveItems(ids: string[], parentId: string | null, index: number): boolean {
    if (ids.length === 0) {
      return false;
    }

    const nodeOpts = ids.map((id) => this.root.getNode(id));

    let nodes: Node<FolderItem>[];
    try {
      // Unwrap early to ensure all nodes are valid
      nodes = nodeOpts.map((n, i) => n.expect("moveItems: Couldn't find node with id " + ids[i]));
    } catch (err) {
      console.error(err);
      return false;
    }

    const parent = parentId ? this.root.getNode(parentId) : Some(this.root.root());
    if (parent.isNone()) {
      return false;
    }

    const parentNode = parent.unwrap();

    // Move items to a temporary node in case nodes are moving inside their current parent
    const tempNode = this.root.createChild(uuidv4(), 0);
    for (const node of nodes.toReversed()) {
      node.moveTo(tempNode, 0);
    }

    for (const node of nodes.toReversed()) {
      node.moveTo(parentNode, index);
    }

    tempNode.delete(DeleteStrategy.Cascade);

    return true;
  }

  getDescendants(id: string | null, order = TraversalOrder.BreadthFirst): Node<FolderItem>[] {
    const node = id ? this.root.getNode(id) : Some(this.root.root());
    if (node.isNone()) {
      return [];
    }

    return node.unwrap().descendants(order);
  }
}

if (import.meta.vitest) {
  const { test, expect } = import.meta.vitest;

  test("WorkspaceFolder", () => {
    const folder = WorkspaceFolder.fromJS({});

    folder.createFolder("f1", "Folder 1", null);
    folder.createFolder("f2", "Folder 2", null);
    folder.createRunbook("i1", "f1");
    folder.createRunbook("i2", "f2");
    folder.createFolder("f3", "Folder 3", "f1");
    folder.createRunbook("i3", "f3");

    const data = folder.toArborist();

    expect(data).toEqual([
      {
        id: "f2",
        name: "Folder 2",
        type: "folder",
        children: [
          {
            id: "i2",
            name: "i2",
            type: "runbook",
          },
        ],
      },
      {
        id: "f1",
        name: "Folder 1",
        type: "folder",
        children: [
          {
            id: "f3",
            name: "Folder 3",
            type: "folder",
            children: [
              {
                id: "i3",
                name: "i3",
                type: "runbook",
              },
            ],
          },
          {
            id: "i1",
            name: "i1",
            type: "runbook",
          },
        ],
      },
    ]);
  });
}
