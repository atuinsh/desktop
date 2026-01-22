import { BlockInfo } from "@/rs-bindings/BlockInfo";

export interface BlockRegistryDefinition {
  typeName: string;
  friendlyName: string;
  shortDescription: string;
  description: string;
}

export default class AIBlockRegistry {
  private static instance: AIBlockRegistry;
  private blocks: Map<string, BlockRegistryDefinition> = new Map();

  private constructor() { }

  public static getInstance(): AIBlockRegistry {
    if (!AIBlockRegistry.instance) {
      AIBlockRegistry.instance = new AIBlockRegistry();
    }
    return AIBlockRegistry.instance;
  }

  public addBlock(block: BlockRegistryDefinition) {
    this.blocks.set(block.typeName, block);
  }

  public getBlockDocs(blockType: string): string {
    const block = this.blocks.get(blockType);
    if (!block) {
      return `No documentation found for block type: ${blockType}`;
    }

    return (
      "Docs for '" +
      blockType +
      "' block (known to users as: " +
      block.friendlyName +
      "):\n" +
      block.description
    );
  }

  public getBlockInfos(): Array<BlockInfo> {
    return Array.from(this.blocks.values()).map((block) => ({
      typeName: block.typeName,
      friendlyName: block.friendlyName,
      summary: block.shortDescription,
      docs: block.description,
    }));
  }
}
