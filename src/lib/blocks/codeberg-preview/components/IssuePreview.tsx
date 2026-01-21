import { Card, CardBody, Chip, Avatar } from "@heroui/react";
import { CircleDotIcon, ExternalLinkIcon, CheckCircleIcon } from "lucide-react";
import type { CodebergIssueData } from "../api";
import { open } from "@tauri-apps/plugin-shell";

interface IssuePreviewProps {
  data: CodebergIssueData;
}

// Normalize color to ensure no duplicate # prefix
function normalizeColor(color: string): string {
  return color.replace(/^#/, "");
}

export default function IssuePreview({ data }: IssuePreviewProps) {
  const handleClick = () => {
    open(data.html_url);
  };

  const getStatusColor = (): "success" | "secondary" => {
    return data.state === "open" ? "success" : "secondary";
  };

  const getStatusIcon = () => {
    return data.state === "open" ? <CircleDotIcon size={14} /> : <CheckCircleIcon size={14} />;
  };

  const getStatusText = () => {
    return data.state === "open" ? "Open" : "Closed";
  };

  return (
    <Card
      className="w-full cursor-pointer hover:bg-default-100 transition-colors"
      shadow="sm"
      isPressable
      onPress={handleClick}
    >
      <CardBody className="p-4 gap-3">
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-3 min-w-0">
            <Avatar src={data.user.avatar_url} size="sm" className="flex-shrink-0 mt-0.5" />
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-semibold text-sm">{data.title}</span>
                <ExternalLinkIcon size={14} className="text-default-400 flex-shrink-0" />
              </div>
              <div className="flex items-center gap-2 mt-1">
                <span className="text-xs text-default-400">#{data.number}</span>
                <span className="text-xs text-default-400">by {data.user.login}</span>
              </div>
            </div>
          </div>
          <Chip
            size="sm"
            variant="flat"
            color={getStatusColor()}
            startContent={getStatusIcon()}
            className="flex-shrink-0"
          >
            {getStatusText()}
          </Chip>
        </div>

        {data.body && (
          <p className="text-sm text-default-500 line-clamp-2">{data.body}</p>
        )}

        <div className="flex items-center gap-3 flex-wrap">
          {data.labels.map((label) => {
            const color = normalizeColor(label.color);
            return (
              <Chip
                key={label.name}
                size="sm"
                variant="flat"
                style={{
                  backgroundColor: `#${color}20`,
                  color: `#${color}`,
                  borderColor: `#${color}40`,
                }}
                className="border"
              >
                {label.name}
              </Chip>
            );
          })}
          {data.comments > 0 && (
            <span className="text-xs text-default-400">
              {data.comments} comment{data.comments !== 1 ? "s" : ""}
            </span>
          )}
        </div>
      </CardBody>
    </Card>
  );
}
