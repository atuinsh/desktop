import { Card, CardBody, Chip, Avatar } from "@heroui/react";
import { CircleDotIcon, ExternalLinkIcon, CheckCircleIcon } from "lucide-react";
import type { GitLabIssueData } from "../api";
import { open } from "@tauri-apps/plugin-shell";

interface IssuePreviewProps {
  data: GitLabIssueData;
}

export default function IssuePreview({ data }: IssuePreviewProps) {
  const handleClick = () => {
    open(data.web_url);
  };

  const getStatusColor = (): "success" | "secondary" => {
    return data.state === "opened" ? "success" : "secondary";
  };

  const getStatusIcon = () => {
    return data.state === "opened" ? <CircleDotIcon size={14} /> : <CheckCircleIcon size={14} />;
  };

  const getStatusText = () => {
    return data.state === "opened" ? "Open" : "Closed";
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
            <Avatar src={data.author.avatar_url} size="sm" className="flex-shrink-0 mt-0.5" />
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-semibold text-sm">{data.title}</span>
                <ExternalLinkIcon size={14} className="text-default-400 flex-shrink-0" />
              </div>
              <div className="flex items-center gap-2 mt-1">
                <span className="text-xs text-default-400">#{data.iid}</span>
                <span className="text-xs text-default-400">by {data.author.username}</span>
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

        {data.description && (
          <p className="text-sm text-default-500 line-clamp-2">{data.description}</p>
        )}

        <div className="flex items-center gap-3 flex-wrap">
          {data.labels.map((label) => (
            <Chip
              key={label}
              size="sm"
              variant="flat"
            >
              {label}
            </Chip>
          ))}
          {data.user_notes_count > 0 && (
            <span className="text-xs text-default-400">
              {data.user_notes_count} comment{data.user_notes_count !== 1 ? "s" : ""}
            </span>
          )}
        </div>
      </CardBody>
    </Card>
  );
}
