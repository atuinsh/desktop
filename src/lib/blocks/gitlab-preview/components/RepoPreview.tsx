import { Card, CardBody, Avatar } from "@heroui/react";
import { StarIcon, GitForkIcon, ExternalLinkIcon } from "lucide-react";
import type { GitLabRepoData } from "../api";
import { open } from "@tauri-apps/plugin-shell";

interface RepoPreviewProps {
  data: GitLabRepoData;
}

export default function RepoPreview({ data }: RepoPreviewProps) {
  const handleClick = () => {
    open(data.web_url);
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
          <div className="flex items-center gap-3 min-w-0">
            <Avatar src={data.owner.avatar_url} name={data.owner.username} size="sm" className="flex-shrink-0" />
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="font-semibold text-sm truncate">{data.path_with_namespace}</span>
                <ExternalLinkIcon size={14} className="text-default-400 flex-shrink-0" />
              </div>
            </div>
          </div>
        </div>

        {data.description && (
          <p className="text-sm text-default-500 line-clamp-2">{data.description}</p>
        )}

        <div className="flex items-center gap-4 text-sm text-default-400">
          <span className="flex items-center gap-1">
            <StarIcon size={14} />
            {formatNumber(data.star_count)}
          </span>
          <span className="flex items-center gap-1">
            <GitForkIcon size={14} />
            {formatNumber(data.forks_count)}
          </span>
        </div>
      </CardBody>
    </Card>
  );
}

function formatNumber(num: number): string {
  if (num >= 1000000) {
    return (num / 1000000).toFixed(1) + "M";
  }
  if (num >= 1000) {
    return (num / 1000).toFixed(1) + "k";
  }
  return num.toString();
}
