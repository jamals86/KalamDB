import Link from 'next/link';
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card';
import { MessageSquare, Sparkles, Zap } from 'lucide-react';

export default function HomePage() {
  const designs = [
    {
      href: '/design1',
      title: 'Classic Chat',
      description: 'Clean ChatGPT-inspired design with sidebar conversations and smooth message streaming.',
      icon: MessageSquare,
      color: 'from-blue-500 to-blue-600',
      preview: 'Light theme, sidebar layout, rounded bubbles',
    },
    {
      href: '/design2',
      title: 'Modern Minimal',
      description: 'Minimalist design with centered layout, soft gradients, and elegant animations.',
      icon: Sparkles,
      color: 'from-purple-500 to-pink-500',
      preview: 'Clean lines, pastel accents, floating cards',
    },
    {
      href: '/design3',
      title: 'Dark Futuristic',
      description: 'Cyberpunk-inspired dark theme with neon accents and terminal-style message rendering.',
      icon: Zap,
      color: 'from-emerald-400 to-cyan-500',
      preview: 'Dark mode, neon glow, mono fonts',
    },
  ];

  return (
    <main className="min-h-screen bg-gradient-to-b from-background to-muted/30 flex items-center justify-center p-4">
      <div className="max-w-4xl w-full space-y-8">
        <div className="text-center space-y-3">
          <h1 className="text-4xl font-bold tracking-tight">
            KalamDB Chat with AI
          </h1>
          <p className="text-muted-foreground text-lg max-w-2xl mx-auto">
            Real-time chat powered by KalamDB topics, live queries, and the kalam-link SDK.
            Choose a design variant to explore.
          </p>
        </div>

        <div className="grid md:grid-cols-3 gap-6">
          {designs.map((design) => (
            <Link key={design.href} href={design.href} className="group">
              <Card className="h-full transition-all duration-300 hover:shadow-lg hover:-translate-y-1 border-2 hover:border-primary/20">
                <CardHeader>
                  <div className={`w-12 h-12 rounded-lg bg-gradient-to-br ${design.color} flex items-center justify-center mb-3 group-hover:scale-110 transition-transform`}>
                    <design.icon className="h-6 w-6 text-white" />
                  </div>
                  <CardTitle className="text-xl">{design.title}</CardTitle>
                  <CardDescription>{design.description}</CardDescription>
                </CardHeader>
                <CardContent>
                  <p className="text-xs text-muted-foreground bg-muted/50 rounded-md px-3 py-2">
                    {design.preview}
                  </p>
                </CardContent>
              </Card>
            </Link>
          ))}
        </div>

        <div className="text-center">
          <p className="text-sm text-muted-foreground">
            Run <code className="bg-muted px-1.5 py-0.5 rounded text-xs">./setup.sh</code> first to initialize the database, then <code className="bg-muted px-1.5 py-0.5 rounded text-xs">npm run service</code> to start the AI processor.
          </p>
        </div>
      </div>
    </main>
  );
}
