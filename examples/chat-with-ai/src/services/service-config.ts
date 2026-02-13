import fs from 'node:fs';
import path from 'node:path';
import dotenv from 'dotenv';

function loadEnvironmentFiles(): void {
  const candidatePaths = [
    path.resolve(process.cwd(), '.env.local'),
    path.resolve(process.cwd(), '.env'),
    path.resolve(__dirname, '../../.env.local'),
    path.resolve(__dirname, '../../.env'),
  ];

  const uniqueExistingPaths = [...new Set(candidatePaths)].filter((filePath) =>
    fs.existsSync(filePath),
  );

  for (const filePath of uniqueExistingPaths) {
    dotenv.config({ path: filePath, override: false });
  }
}

loadEnvironmentFiles();

export interface ServiceConfig {
  kalamdbUrl: string;
  kalamdbUsername: string;
  kalamdbPassword: string;
  topicName: string;
  consumerGroup: string;
  batchSize: number;
  geminiApiKey: string;
  geminiModel: string;
  aiSystemPrompt: string;
  aiMaxOutputTokens: number;
  aiTemperature: number;
  aiContextWindowMessages: number;
}

const DEFAULT_SYSTEM_PROMPT = `You are AI Assistant in a real-time chat app powered by KalamDB.
Write concise, helpful, and accurate responses.
If you are unsure, say what you are unsure about and ask a follow-up question.`;

function requiredEnv(name: string, env: NodeJS.ProcessEnv): string {
  const value = env[name];
  if (!value || value.trim().length === 0) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

function numberEnv(
  name: string,
  env: NodeJS.ProcessEnv,
  defaultValue: number,
): number {
  const rawValue = env[name];
  if (!rawValue || rawValue.trim().length === 0) {
    return defaultValue;
  }

  const parsedValue = Number(rawValue);
  if (!Number.isFinite(parsedValue)) {
    throw new Error(`Invalid numeric value for ${name}: ${rawValue}`);
  }

  return parsedValue;
}

export function loadServiceConfig(env: NodeJS.ProcessEnv = process.env): ServiceConfig {
  const geminiApiKey = env.GEMINI_API_KEY?.trim() || env.GOOGLE_GENERATIVE_AI_API_KEY?.trim();
  if (!geminiApiKey) {
    throw new Error(
      'Missing required environment variable: GEMINI_API_KEY or GOOGLE_GENERATIVE_AI_API_KEY',
    );
  }

  return {
    kalamdbUrl: env.KALAMDB_URL || 'http://localhost:8080',
    kalamdbUsername: requiredEnv('KALAMDB_USERNAME', env),
    kalamdbPassword: requiredEnv('KALAMDB_PASSWORD', env),
    topicName: env.KALAMDB_TOPIC || 'chat.ai-processing',
    consumerGroup: env.KALAMDB_CONSUMER_GROUP || 'ai-processor-service',
    batchSize: numberEnv('KALAMDB_BATCH_SIZE', env, 1),
    geminiApiKey,
    geminiModel: env.GEMINI_MODEL || 'gemini-2.5-flash',
    aiSystemPrompt: env.AI_SYSTEM_PROMPT || DEFAULT_SYSTEM_PROMPT,
    aiMaxOutputTokens: numberEnv('AI_MAX_OUTPUT_TOKENS', env, 512),
    aiTemperature: numberEnv('AI_TEMPERATURE', env, 0.7),
    aiContextWindowMessages: numberEnv('AI_CONTEXT_WINDOW_MESSAGES', env, 20),
  };
}
