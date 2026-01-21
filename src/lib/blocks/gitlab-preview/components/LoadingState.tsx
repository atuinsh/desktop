import { Card, CardBody, Skeleton } from "@heroui/react";

export default function LoadingState() {
  return (
    <Card className="w-full" shadow="sm">
      <CardBody className="p-4 gap-3">
        <div className="flex items-center gap-3">
          <Skeleton className="w-10 h-10 rounded-full" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-4 w-3/4 rounded" />
            <Skeleton className="h-3 w-1/2 rounded" />
          </div>
        </div>
        <Skeleton className="h-3 w-full rounded" />
        <Skeleton className="h-3 w-2/3 rounded" />
      </CardBody>
    </Card>
  );
}
