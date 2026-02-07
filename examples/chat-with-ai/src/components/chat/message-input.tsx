'use client';

import { useState, useRef, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Send, Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';
import { FileUpload } from './file-upload';
import { EmojiPickerButton } from './emoji-picker-button';
import type { UploadProgress } from 'kalam-link';

interface MessageInputProps {
  onSend: (content: string, files?: File[]) => Promise<void>;
  onTypingChange?: (isTyping: boolean) => void;
  disabled?: boolean;
  sending?: boolean;
  uploadProgress?: UploadProgress | null;
  placeholder?: string;
  className?: string;
}

export function MessageInput({
  onSend,
  onTypingChange,
  disabled = false,
  sending = false,
  uploadProgress = null,
  placeholder = 'Type a message...',
  className,
}: MessageInputProps) {
  const [content, setContent] = useState('');
  const [files, setFiles] = useState<File[]>([]);
  const typingTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const isTypingRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleTyping = useCallback(() => {
    if (!isTypingRef.current) {
      isTypingRef.current = true;
      onTypingChange?.(true);
    }

    // Clear previous timeout
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    // Set new timeout to clear typing after 2 seconds of inactivity
    typingTimeoutRef.current = setTimeout(() => {
      isTypingRef.current = false;
      onTypingChange?.(false);
    }, 2000);
  }, [onTypingChange]);

  const handleSend = useCallback(async () => {
    const trimmed = content.trim();
    if ((!trimmed && files.length === 0) || disabled || sending) return;

    // Clear typing
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }
    isTypingRef.current = false;
    onTypingChange?.(false);

    const messageContent = trimmed;
    const messageFiles = [...files];
    setContent('');
    setFiles([]);
    await onSend(messageContent, messageFiles);
    
    // Focus back on textarea
    textareaRef.current?.focus();
  }, [content, files, disabled, sending, onSend, onTypingChange]);

  const handleEmojiSelect = useCallback((emoji: string) => {
    setContent(prev => prev + emoji);
    textareaRef.current?.focus();
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  return (
    <div className={cn('space-y-2', className)}>
      {/* File Attachments */}
      <FileUpload
        files={files}
        onFilesChange={setFiles}
        disabled={disabled || sending}
        maxFiles={10}
        maxSizeMB={10}
        uploadProgress={uploadProgress}
      />

      {/* Input Area */}
      <div className="flex items-end gap-2">
        <Textarea
          ref={textareaRef}
          value={content}
          onChange={(e) => {
            setContent(e.target.value);
            handleTyping();
          }}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          disabled={disabled || sending}
          rows={1}
          className="min-h-[44px] max-h-[200px] resize-none"
        />
        <EmojiPickerButton
          onEmojiSelect={handleEmojiSelect}
          disabled={disabled || sending}
        />
        <Button
          onClick={handleSend}
          disabled={(!content.trim() && files.length === 0) || disabled || sending}
          size="icon"
          className="h-[44px] w-[44px] shrink-0"
        >
          {sending ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Send className="h-4 w-4" />
          )}
        </Button>
      </div>
    </div>
  );
}
