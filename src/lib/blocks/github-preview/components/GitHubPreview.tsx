import { useEffect, useState, useCallback, useRef } from "react";
import { Card, CardBody, Button } from "@heroui/react";
import { RefreshCwIcon, AlertCircleIcon } from "lucide-react";
import LoadingState from "./LoadingState";
import RepoPreview from "./RepoPreview";
import PRPreview from "./PRPreview";
import IssuePreview from "./IssuePreview";
import CodePreview from "./CodePreview";
import { fetchRepoData, fetchPRData, fetchIssueData, fetchCodeData, RepoData, PRData, IssueData, CodeData } from "../api";
import type { GitHubBlockProps } from "../schema";

const CACHE_DURATION_MS = 60 * 60 * 1000; // 1 hour

interface GitHubPreviewProps {
  props: GitHubBlockProps;
  updateProps: (updates: Partial<GitHubBlockProps>) => void;
}

export default function GitHubPreview({ props, updateProps }: GitHubPreviewProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [data, setData] = useState<RepoData | PRData | IssueData | CodeData | null>(null);
  const mountedRef = useRef(true);

  // Check if cache is still valid
  const isCacheValid = useCallback(() => {
    if (!props.cachedAt || !props.cachedData || props.cachedData === "{}") {
      return false;
    }
    const cachedTime = new Date(props.cachedAt).getTime();
    return Date.now() - cachedTime < CACHE_DURATION_MS;
  }, [props.cachedAt, props.cachedData]);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      let result: RepoData | PRData | IssueData | CodeData;

      switch (props.urlType) {
        case "repo":
          result = await fetchRepoData(props.owner, props.repo);
          break;
        case "pr":
          result = await fetchPRData(props.owner, props.repo, parseInt(props.prNumber, 10));
          break;
        case "issue":
          result = await fetchIssueData(props.owner, props.repo, parseInt(props.issueNumber, 10));
          break;
        case "code":
          result = await fetchCodeData(
            props.owner,
            props.repo,
            props.branch,
            props.filePath,
            props.lineStart ? parseInt(props.lineStart, 10) : undefined,
            props.lineEnd ? parseInt(props.lineEnd, 10) : undefined,
          );
          break;
        default:
          throw new Error(`Unknown URL type: ${props.urlType}`);
      }

      if (!mountedRef.current) return;

      setData(result);

      // Cache the result
      updateProps({
        cachedData: JSON.stringify(result),
        cachedAt: new Date().toISOString(),
      });
    } catch (err) {
      if (!mountedRef.current) return;
      setError(err instanceof Error ? err.message : "Failed to fetch data");
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, [props.owner, props.repo, props.urlType, props.prNumber, props.issueNumber, props.branch, props.filePath, props.lineStart, props.lineEnd, updateProps]);

  // Load data from cache or fetch
  useEffect(() => {
    mountedRef.current = true;

    if (isCacheValid()) {
      try {
        setData(JSON.parse(props.cachedData));
        return;
      } catch {
        // Cache is invalid, will fetch fresh
      }
    }

    if (props.owner && props.repo && props.urlType) {
      fetchData();
    }

    return () => {
      mountedRef.current = false;
    };
  }, [props.owner, props.repo, props.urlType, props.prNumber, props.issueNumber, props.branch, props.filePath, isCacheValid, props.cachedData, fetchData]);

  const handleRefresh = () => {
    fetchData();
  };

  if (loading) {
    return <LoadingState />;
  }

  if (error) {
    return (
      <Card className="w-full" shadow="sm">
        <CardBody className="p-4">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 text-danger">
              <AlertCircleIcon size={18} />
              <span className="text-sm">{error}</span>
            </div>
            <Button size="sm" variant="flat" onPress={handleRefresh} startContent={<RefreshCwIcon size={14} />}>
              Retry
            </Button>
          </div>
        </CardBody>
      </Card>
    );
  }

  if (!data) {
    return <LoadingState />;
  }

  return (
    <div className="relative group">
      {props.urlType === "repo" && <RepoPreview data={data as RepoData} />}
      {props.urlType === "pr" && <PRPreview data={data as PRData} />}
      {props.urlType === "issue" && <IssuePreview data={data as IssueData} />}
      {props.urlType === "code" && <CodePreview data={data as CodeData} />}

      {/* Refresh button - shows on hover */}
      <Button
        size="sm"
        variant="flat"
        isIconOnly
        className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity"
        onPress={handleRefresh}
      >
        <RefreshCwIcon size={14} />
      </Button>
    </div>
  );
}
