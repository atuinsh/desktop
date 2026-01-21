import { Card, CardBody, Chip, Avatar } from "@heroui/react";
import { GitPullRequestIcon, ExternalLinkIcon, GitMergeIcon, XCircleIcon } from "lucide-react";
import type { PRData } from "../api";
import { open } from "@tauri-apps/plugin-shell";

interface PRPreviewProps {
  data: PRData;
}

export default function PRPreview({ data }: PRPreviewProps) {
  const handleClick = () => {
    open(data.html_url);
  };

  const getStatusColor = (): "success" | "secondary" | "danger" => {
    if (data.merged) return "secondary";
    if (data.state === "open") return "success";
    return "danger";
  };

  const getStatusIcon = () => {
    if (data.merged) return <GitMergeIcon size={14} />;
    if (data.state === "open") return <GitPullRequestIcon size={14} />;
    return <XCircleIcon size={14} />;
  };

  const getStatusText = () => {
    if (data.merged) return "Merged";
    if (data.state === "open") return "Open";
    return "Closed";
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

        <div className="flex items-center gap-4 text-sm">
          <span className="text-success-500">+{data.additions}</span>
          <span className="text-danger-500">-{data.deletions}</span>
          <span className="text-default-400">{data.changed_files} files</span>
        </div>
      </CardBody>
    </Card>
  );
}
