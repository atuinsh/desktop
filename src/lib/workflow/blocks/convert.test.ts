import {  expect, test } from "vitest";
import { ScriptBlock } from "./script";
import { convertBlocknoteToAtuin } from "./convert";
import { ClickhouseBlock } from "./clickhouse";
import { HttpBlock } from "@/lib/blocks/http/schema";
import { PrometheusBlock } from "./prometheus";
import { SQLiteBlock } from "./sqlite";
import { PostgresBlock } from "./postgres";
import { MySqlBlock } from "./mysql";
import { TerminalBlock } from "@/lib/blocks/terminal";

test("TerminalBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "run",
        props: {
            name: "run-block",
            code: "echo 'Hello, world!'",
            outputVisible: true,
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as TerminalBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("run-block");
    expect(block.code).toBe("echo 'Hello, world!'");
    expect(block.outputVisible).toBe(true);
});

test("ScriptBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "script",
        props: {
            name: "script-block",
            code: "echo 'Hello, world!'",
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as ScriptBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("script-block");
    expect(block.code).toBe("echo 'Hello, world!'");
});

test("SQLiteBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "sqlite",
        props: {
            name: "sqlite-block",
            query: "SELECT * FROM users",
            uri: "sqlite://foo/bar",
            autoRefresh: 1000,
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as SQLiteBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("sqlite-block");
    expect(block.query).toBe("SELECT * FROM users");
    expect(block.uri).toBe("sqlite://foo/bar");
    expect(block.autoRefresh).toBe(1000);
});

test("ClickhouseBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "clickhouse",
        props: {
            name: "clickhouse-block",
            query: "SELECT * FROM users",
            uri: "clickhouse://foo/bar",
            autoRefresh: 1000,
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as ClickhouseBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("clickhouse-block");
    expect(block.query).toBe("SELECT * FROM users");
    expect(block.uri).toBe("clickhouse://foo/bar");
    expect(block.autoRefresh).toBe(1000);
});

test("PostgresBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "postgres",
        props: {
            name: "postgres-block",
            query: "SELECT * FROM users",
            uri: "postgres://foo/bar",
            autoRefresh: 1000,
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as PostgresBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("postgres-block");
    expect(block.query).toBe("SELECT * FROM users");
    expect(block.uri).toBe("postgres://foo/bar");
    expect(block.autoRefresh).toBe(1000);
}); 

test("MySqlBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "mysql",
        props: {
            name: "mysql-block",
            query: "SELECT * FROM users",
            uri: "mysql://foo/bar",
            autoRefresh: 1000,
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as MySqlBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("mysql-block");
    expect(block.query).toBe("SELECT * FROM users");
    expect(block.uri).toBe("mysql://foo/bar");
    expect(block.autoRefresh).toBe(1000);
});

test("HttpBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "http",
        props: {
            name: "http-block",
            url: "http://foo/bar",
            verb: "GET",
            headers: JSON.stringify({
                "Content-Type": "application/json",
            }),
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as HttpBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("http-block");
    expect(block.url).toBe("http://foo/bar");
    expect(block.verb).toBe("GET");
    expect(block.headers).toEqual({
        "Content-Type": "application/json",
    });
});

test("PrometheusBlock can be created from blocknote", () => {
    let bnb = {
        id: "b339887e-746a-4d2c-b344-cee575cda742",
        type: "prometheus",
        props: {
            name: "prometheus-block",
            query: "sum(rate(http_requests_total[1m]))",
            endpoint: "http://foo/bar",
            period: "1m",
            autoRefresh: true,
        },
        children: [],
    };

    let block = convertBlocknoteToAtuin(bnb) as PrometheusBlock;
    expect(block).toBeDefined();
    expect(block.id).toBe("b339887e-746a-4d2c-b344-cee575cda742");
    expect(block.name).toBe("prometheus-block");
    expect(block.query).toBe("sum(rate(http_requests_total[1m]))");
    expect(block.endpoint).toBe("http://foo/bar");
    expect(block.period).toBe("1m");
    expect(block.autoRefresh).toBe(true);
});

