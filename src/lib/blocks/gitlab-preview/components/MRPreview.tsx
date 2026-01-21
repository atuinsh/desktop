import { Card, CardBody, Chip, Avatar } from "@heroui/react";
import { GitPullRequestIcon, ExternalLinkIcon, GitMergeIcon, XCircleIcon } from "lucide-react";
import type { GitLabMRData } from "../api";
import { open } from "@tauri-apps/plugin-shell";

interface MRPreviewProps {
  data: GitLabMRData;
}

export default function MRPreview({ data }: MRPreviewProps) {
  const handleClick = () => {
    open(data.web_url);
  };

  const getStatusColor = (): "success" | "secondary" | "danger" => {
    if (data.state === "merged") return "secondary";
    if (data.state === "opened") return "success";
    return "danger";
  };

  const getStatusIcon = () => {
    if (data.state === "merged") return <GitMergeIcon size={14} />;
    if (data.state === "opened") return <GitPullRequestIcon size={14} />;
    return <XCircleIcon size={14} />;
  };

  const getStatusText = () => {
    if (data.state === "merged") return "Merged";
    if (data.state === "opened") return "Open";
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
            <Avatar src={data.author.avatar_url} size="sm" className="flex-shrink-0 mt-0.5" />
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-semibold text-sm">{data.title}</span>
                <ExternalLinkIcon size={14} className="text-default-400 flex-shrink-0" />
              </div>
              <div className="flex items-center gap-2 mt-1">
                <span className="text-xs text-default-400">!{data.iid}</span>
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

        {data.changes_count && (
          <div className="flex items-center gap-4 text-sm text-default-400">
            <span>{data.changes_count} changes</span>
          </div>
        )}
      </CardBody>
    </Card>
  );
}
