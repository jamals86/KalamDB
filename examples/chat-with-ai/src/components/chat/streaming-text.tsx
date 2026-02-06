'use client';

import { useEffect, useRef, useState } from 'react';

interface StreamingTextProps {
  text: string;
  isNew: boolean;
  speed?: number;
  className?: string;
  onComplete?: () => void;
}

/**
 * Renders text with a token-by-token streaming animation
 * Only animates if `isNew` is true (for newly arrived messages)
 */
export function StreamingText({
  text,
  isNew,
  speed = 20,
  className,
  onComplete,
}: StreamingTextProps) {
  const [displayedText, setDisplayedText] = useState(isNew ? '' : text);
  const [cursor, setCursor] = useState(isNew);
  const indexRef = useRef(0);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);

  useEffect(() => {
    if (!isNew) {
      setDisplayedText(text);
      setCursor(false);
      return;
    }

    indexRef.current = 0;
    setDisplayedText('');
    setCursor(true);

    intervalRef.current = setInterval(() => {
      indexRef.current++;

      if (indexRef.current >= text.length) {
        setDisplayedText(text);
        setCursor(false);
        onComplete?.();
        if (intervalRef.current) clearInterval(intervalRef.current);
        return;
      }

      // Try to break on word boundaries for natural feel
      let end = indexRef.current;
      const nextSpace = text.indexOf(' ', end);
      if (nextSpace !== -1 && nextSpace - end < 6) {
        end = nextSpace + 1;
      } else {
        end += 1;
      }
      indexRef.current = Math.min(end, text.length);
      setDisplayedText(text.substring(0, indexRef.current));
    }, speed);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [text, isNew, speed, onComplete]);

  return (
    <span className={className}>
      {displayedText}
      {cursor && (
        <span className="inline-block w-[2px] h-[1em] bg-current ml-0.5 animate-pulse" />
      )}
    </span>
  );
}
