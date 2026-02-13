import { createGoogleGenerativeAI } from '@ai-sdk/google';
import { generateText, streamText } from 'ai';

import type { ServiceConfig } from './service-config';

export interface ConversationTurn {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

interface GenerateAIResponseParams {
  userMessage: string;
  history: ConversationTurn[];
  config: ServiceConfig;
}

interface GenerateAIStreamParams extends GenerateAIResponseParams {
  onTextDelta?: (delta: string, aggregateText: string) => Promise<void> | void;
  onTokenProgress?: (tokenCount: number) => Promise<void> | void;
}

interface GenerateConversationTitleParams {
  userMessage: string;
  assistantReply: string;
  history: ConversationTurn[];
  config: ServiceConfig;
}

export interface StreamedAIResponse {
  text: string;
  tokenCount: number;
}

const providerCache = new Map<string, ReturnType<typeof createGoogleGenerativeAI>>();

function getProvider(apiKey: string): ReturnType<typeof createGoogleGenerativeAI> {
  const cached = providerCache.get(apiKey);
  if (cached) {
    return cached;
  }

  const provider = createGoogleGenerativeAI({ apiKey });
  providerCache.set(apiKey, provider);
  return provider;
}

function buildPrompt(history: ConversationTurn[], userMessage: string): string {
  const transcript = history
    .filter(message => message.content.trim().length > 0)
    .map(message => `${message.role.toUpperCase()}: ${message.content.trim()}`)
    .join('\n\n');

  if (transcript.length === 0) {
    return `User message:\n${userMessage}`;
  }

  return `Conversation transcript:\n${transcript}\n\nUser message:\n${userMessage}`;
}

function countApproxTokens(text: string): number {
  const normalized = text.trim();
  if (normalized.length === 0) {
    return 0;
  }
  return normalized.split(/\s+/).length;
}

function normalizeTitle(raw: string): string {
  const noQuotes = raw.replace(/^['"`]+|['"`]+$/g, '').trim();
  const singleLine = noQuotes.replace(/\s+/g, ' ').trim();
  if (!singleLine) {
    return 'New conversation';
  }
  return singleLine.slice(0, 80);
}

export async function generateAIResponse({
  userMessage,
  history,
  config,
}: GenerateAIResponseParams): Promise<string> {
  const google = getProvider(config.geminiApiKey);

  const { text } = await generateText({
    model: google(config.geminiModel),
    system: config.aiSystemPrompt,
    prompt: buildPrompt(history, userMessage),
    maxOutputTokens: config.aiMaxOutputTokens,
    temperature: config.aiTemperature,
  });

  const trimmed = text.trim();
  if (!trimmed) {
    throw new Error('Gemini returned an empty response');
  }

  return trimmed;
}

export async function generateAIResponseStream({
  userMessage,
  history,
  config,
  onTextDelta,
  onTokenProgress,
}: GenerateAIStreamParams): Promise<StreamedAIResponse> {
  const google = getProvider(config.geminiApiKey);

  const result = streamText({
    model: google(config.geminiModel),
    system: config.aiSystemPrompt,
    prompt: buildPrompt(history, userMessage),
    maxOutputTokens: config.aiMaxOutputTokens,
    temperature: config.aiTemperature,
  });

  let aggregateText = '';
  let tokenCount = 0;

  for await (const delta of result.textStream) {
    if (!delta) {
      continue;
    }
    aggregateText += delta;
    tokenCount = countApproxTokens(aggregateText);
    await onTextDelta?.(delta, aggregateText);
    await onTokenProgress?.(tokenCount);
  }

  const trimmed = aggregateText.trim();
  if (!trimmed) {
    throw new Error('Gemini returned an empty streamed response');
  }

  return {
    text: trimmed,
    tokenCount,
  };
}

export async function generateConversationTitle({
  userMessage,
  assistantReply,
  history,
  config,
}: GenerateConversationTitleParams): Promise<string> {
  const google = getProvider(config.geminiApiKey);

  const recentContext = history
    .slice(-4)
    .map((turn) => `${turn.role.toUpperCase()}: ${turn.content}`)
    .join('\n');

  const { text } = await generateText({
    model: google(config.geminiModel),
    temperature: 0.2,
    maxOutputTokens: 24,
    system: 'Generate short chat conversation titles. Output only the title, no quotes, no markdown.',
    prompt: `Create a concise title (3-8 words) for this conversation based on the latest exchange.\n\nRecent context:\n${recentContext || 'N/A'}\n\nLatest user message:\n${userMessage}\n\nLatest assistant reply:\n${assistantReply}`,
  });

  return normalizeTitle(text);
}
