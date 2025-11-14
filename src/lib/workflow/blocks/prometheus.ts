import Block from "./block";
import { DependencySpec } from "../dependency";

export class PrometheusBlock extends Block {
  query: string;
  endpoint: string;
  period: string;
  autoRefresh: number;

  get typeName() {
    return "prometheus";
  }

  constructor(
    id: string,
    name: string = "Prometheus",
    dependency: DependencySpec,
    query: string = "",
    endpoint: string = "",
    period: string = "1h",
    autoRefresh: number = 0,
  ) {
    super(id, name, dependency);

    this.query = query;
    this.endpoint = endpoint;
    this.period = period;
    this.autoRefresh = autoRefresh || 0;
  }

  object() {
    return {
      id: this.id,
      name: this.name,
      query: this.query,
      endpoint: this.endpoint,
      period: this.period,
      autoRefresh: this.autoRefresh,
    };
  }

  serialize() {
    return JSON.stringify(this.object());
  }

  static deserialize(json: string) {
    const data = JSON.parse(json);
    return new PrometheusBlock(
      data.id,
      data.name,
      data.dependency,
      data.query,
      data.endpoint,
      data.period,
      data.autoRefresh,
    );
  }
}
