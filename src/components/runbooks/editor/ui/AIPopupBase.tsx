import { useState, useCallback, useEffect, useRef } from "react";
import { Button, Textarea, Spinner, addToast } from "@heroui/react";
import { SparklesIcon, ArrowRightIcon } from "lucide-react";
import { AIQuotaExceededError } from "@/api/ai";

interface AIPopupBaseProps {
  isVisible: boolean;
  position: { x: number; y: number };
  onClose: () => void;
  onSubmit: (prompt: string) => Promise<void>;
  title: string;
  placeholder: string;
  submitButtonText: string;
  submitButtonLoadingText: string;
  suggestions?: string[];
  showSuggestions?: boolean;
  onSuggestionClick?: (suggestion: string) => void;
}

export function AIPopupBase({
  isVisible,
  position,
  onClose,
  onSubmit,
  title,
  placeholder,
  submitButtonText,
  submitButtonLoadingText,
  suggestions = [],
  showSuggestions = false,
  onSuggestionClick,
}: AIPopupBaseProps) {
  const [prompt, setPrompt] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const popupRef = useRef<HTMLDivElement>(null);

  const handleSubmit = useCallback(async () => {
    if (!prompt.trim()) return;

    setIsLoading(true);
    setError(null);

    try {
      await onSubmit(prompt);
      setPrompt("");
      onClose();
    } catch (err) {
      if (err instanceof AIQuotaExceededError) {
        addToast({
          title: "Quota exceeded",
          description: "AI quota exceeded",
          color: "danger",
        });
        onClose();
      } else {
        const errorMessage = err instanceof Error ? err.message : "Request failed";
        setError(errorMessage);
      }
    } finally {
      setIsLoading(false);
    }
  }, [prompt, onSubmit, onClose]);

  const handleSuggestionClick = (suggestion: string) => {
    onSuggestionClick?.(suggestion);
    setPrompt(suggestion);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  useEffect(() => {
    if (isVisible) {
      setPrompt("");
      setError(null);
      
      // Focus after a brief delay to ensure the popup is rendered
      const timeoutId = setTimeout(() => {
        textareaRef.current?.focus();
      }, 100);
      
      return () => clearTimeout(timeoutId);
    }
  }, [isVisible]);

  const handlePopupClick = useCallback((e: React.MouseEvent) => {
    // Stop event from reaching BlockNote editor which steals focus
    e.stopPropagation();
    
    // If clicking on the popup but not on an interactive element, refocus textarea
    const target = e.target as HTMLElement;
    const isButton = target.closest('button');
    const isTextarea = target.closest('textarea');
    const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA';
    
    if (!isButton && !isTextarea && !isInput && textareaRef.current) {
      e.preventDefault();
      textareaRef.current.focus();
    }
  }, []);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (popupRef.current && !popupRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };

    if (isVisible) {
      document.addEventListener("mousedown", handleClickOutside);
      document.addEventListener("keydown", handleEscape);
    }

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [isVisible, onClose]);

  if (!isVisible) return null;

  // Check if popup would go off-screen at the top
  const popupHeight = showSuggestions ? 300 : 200;
  const shouldPositionBelow = position.y - popupHeight < 0;

  return (
    <div
      ref={popupRef}
      onClick={handlePopupClick}
      onMouseDown={(e) => e.stopPropagation()}
      onMouseUp={(e) => e.stopPropagation()}
      className="absolute z-50 w-2/3 bg-white dark:bg-gray-900 border border-purple-200 dark:border-purple-800 rounded-lg shadow-xl"
      style={{
        left: position.x,
        top: shouldPositionBelow ? position.y + 30 : position.y - 10,
        transform: shouldPositionBelow ? 'none' : 'translateY(-100%)',
      }}
    >
      <div className="bg-gradient-to-r from-purple-50 to-blue-50 dark:from-purple-950/20 dark:to-blue-950/20 p-4 rounded-lg">
        <div className="flex items-start gap-3">
          <div className="flex-shrink-0 mt-1">
            <SparklesIcon className="h-5 w-5 text-purple-600 dark:text-purple-400" />
          </div>
          
          <div className="flex-1 space-y-3">
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium text-purple-900 dark:text-purple-100">
                {title}
              </span>
            </div>
            
            {/* Quick suggestions */}
            {showSuggestions && suggestions.length > 0 && (
              <div className="space-y-2">
                <p className="text-xs text-purple-700 dark:text-purple-300">
                  Quick suggestions:
                </p>
                <div className="flex flex-wrap gap-1">
                  {suggestions.slice(0, 3).map((suggestion, index) => (
                    <Button
                      key={index}
                      size="sm"
                      variant="bordered"
                      onPress={() => handleSuggestionClick(suggestion)}
                      className="text-xs h-6 px-2 border-purple-200 dark:border-purple-700 text-purple-700 dark:text-purple-300 hover:bg-purple-100 dark:hover:bg-purple-900/20"
                    >
                      {suggestion}
                    </Button>
                  ))}
                </div>
              </div>
            )}
            
            <Textarea
              ref={textareaRef}
              placeholder={placeholder}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={handleKeyDown}
              minRows={2}
              maxRows={4}
              disabled={isLoading}
              className="text-sm"
              variant="bordered"
              autoFocus
              classNames={{
                input: "focus:ring-0 focus:outline-none",
                inputWrapper: "focus-within:ring-0 focus-within:outline-none"
              }}
            />
            
            {error && (
              <div className="text-red-600 dark:text-red-400 text-xs bg-red-50 dark:bg-red-950/20 p-2 rounded border border-red-200 dark:border-red-800">
                {error}
              </div>
            )}
            
            <div className="flex items-center justify-between">
              <div className="text-xs text-purple-600 dark:text-purple-400">
                <kbd className="px-1 py-0.5 bg-purple-100 dark:bg-purple-900 rounded text-xs">⌘</kbd> + <kbd className="px-1 py-0.5 bg-purple-100 dark:bg-purple-900 rounded text-xs">Enter</kbd> to {submitButtonText.toLowerCase()} • <kbd className="px-1 py-0.5 bg-purple-100 dark:bg-purple-900 rounded text-xs">Esc</kbd> to cancel
              </div>
              
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="light"
                  onPress={onClose}
                  disabled={isLoading}
                  className="text-purple-600 dark:text-purple-400"
                >
                  Cancel
                </Button>
                <Button
                  size="sm"
                  color="secondary"
                  onPress={handleSubmit}
                  disabled={!prompt.trim() || isLoading}
                  startContent={isLoading ? <Spinner size="sm" /> : <ArrowRightIcon className="h-3 w-3" />}
                  className="bg-purple-600 text-white hover:bg-purple-700"
                >
                  {isLoading ? submitButtonLoadingText : submitButtonText}
                </Button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
