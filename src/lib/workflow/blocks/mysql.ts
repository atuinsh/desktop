import { DependencySpec } from "../dependency";
import Block from "./block";

export class MySqlBlock extends Block {
    query: string;
    uri: string;
    autoRefresh: number;
    skipSqlModeInit: boolean;

    get typeName() {
        return "mysql";
    }

    constructor(id: string, name: string, dependency: DependencySpec, query: string, uri: string, autoRefresh: number, skipSqlModeInit: boolean = false) {
        super(id, name, dependency);

        this.query = query;
        this.uri = uri;
        this.autoRefresh = autoRefresh;
        this.skipSqlModeInit = skipSqlModeInit;
    }

    object() {
        return {
            id: this.id,
            name: this.name,
            query: this.query,
            uri: this.uri,
            autoRefresh: this.autoRefresh,
            skipSqlModeInit: this.skipSqlModeInit,
        };
    }

    serialize() {
        return JSON.stringify(this.object());
    }

    static deserialize(json: string) {
        const data = JSON.parse(json);
        return new MySqlBlock(data.id, data.name, data.dependency, data.query, data.uri, data.autoRefresh, data.skipSqlModeInit ?? false);
    }
}
