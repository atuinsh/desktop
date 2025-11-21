import EditableHeading from "@/components/EditableHeading";
import { Card, CardHeader, CardBody, CardFooter, cn, Tooltip, Chip } from "@heroui/react";
import { default as BlockType } from "@/lib/workflow/blocks/block";
import { DependencySpec } from "@/lib/workflow/dependency";
import { useBlockNoteEditor } from "@blocknote/react";
import { convertBlocknoteToAtuin } from "@/lib/workflow/blocks/convert";
import { useMemo } from "react";
import { WorkflowIcon } from "lucide-react";

interface BlockProps {
  name: string;
  type: string;
  block: BlockType;

  setName: (name: string) => void;
  setDependency: (dependency: DependencySpec) => void;

  header?: any;
  footer?: any;
  children: any;
  inlineHeader?: boolean;
  hideChild?: boolean;
  hasDependency?: boolean;
  ref?: React.RefObject<HTMLDivElement>;
  className?: string;
  bodyClassName?: string;
  topRightElement?: React.ReactNode;
}

export default function Block({
  name,
  type,
  block,
  setName,
  header,
  children,
  footer,
  inlineHeader,
  hideChild,
  hasDependency,
  ref,
  className,
  bodyClassName,
  topRightElement,
}: BlockProps) {
  let editor = useBlockNoteEditor();

  let parentBlock = useMemo(() => {
    if (!block?.dependency?.parent) {
      return null;
    }

    let bnb = editor.document.find((b: any) => b.id === block.dependency.parent);
    if (bnb) {
      let block = convertBlocknoteToAtuin(bnb);
      return block;
    }

    return null;
  }, [editor.document, block?.dependency?.parent]);

  return (
    <Card className={cn("w-full !max-w-full !outline-none", className)} shadow="sm" ref={ref}>
      <CardHeader className="p-3 gap-2 bg-default-50 flex flex-col items-start justify-start z-auto">
        <div className="flex flex-row justify-between w-full">
          <span className="text-default-700 font-semibold text-xs">{type}</span>
          <div className="flex items-center gap-2">
            {topRightElement}
            {hasDependency && parentBlock && (
              <Tooltip content={`This ${type} depends on ${parentBlock?.name}`}>
                <Chip
                  variant="flat"
                  size="sm"
                  className="pl-3 py-2"
                  startContent={<WorkflowIcon size={14} />}
                >
                  {parentBlock?.name}
                </Chip>
              </Tooltip>
            )}
          </div>
        </div>
        {!inlineHeader && (
          <div className="flex flex-row justify-between w-full">
            <EditableHeading initialText={name} onTextChange={setName} />
          </div>
        )}

        {header && header}
      </CardHeader>

      {children && <CardBody className={cn(hideChild ? "hidden" : "", bodyClassName)}>{children}</CardBody>}

      {footer && <CardFooter>{footer}</CardFooter>}
    </Card>
  );
}
