import { useEffect } from 'react';
import { useSettings, Setting } from '@/hooks/useSettings';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Loader2, RefreshCw, Settings as SettingsIcon } from 'lucide-react';

export function SettingsView() {
  const { groupedSettings, isLoading, error, fetchSettings } = useSettings();

  useEffect(() => {
    fetchSettings();
  }, [fetchSettings]);

  if (error) {
    return (
      <Card className="border-red-200">
        <CardContent className="py-6">
          <p className="text-red-700">{error}</p>
          <Button variant="outline" onClick={fetchSettings} className="mt-2">
            Retry
          </Button>
        </CardContent>
      </Card>
    );
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  const categories = Object.keys(groupedSettings);

  if (categories.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>No Settings Available</CardTitle>
          <CardDescription>
            Configuration settings are not available at this time.
          </CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
      {/* Toolbar */}
      <div className="flex items-center justify-end">
        <Button variant="outline" size="icon" onClick={fetchSettings} disabled={isLoading}>
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      {/* Settings by Category */}
      {categories.map((category) => (
        <Card key={category}>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <SettingsIcon className="h-5 w-5" />
              {category}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="divide-y">
              {groupedSettings[category].map((setting: Setting) => (
                <div key={setting.name} className="py-4 first:pt-0 last:pb-0">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <h4 className="font-medium">{setting.name}</h4>
                      <p className="text-sm text-muted-foreground mt-1">
                        {setting.description}
                      </p>
                    </div>
                    <div className="text-right">
                      <code className="px-2 py-1 bg-muted rounded text-sm">
                        {setting.value}
                      </code>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
