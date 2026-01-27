import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Database, Check, Eye, EyeOff, AlertCircle, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useAppDispatch, useAppSelector } from "@/store/hooks";
import { submitSetup, clearSetupError } from "@/store/setupSlice";

interface FormData {
  username: string;
  password: string;
  confirmPassword: string;
  rootPassword: string;
  confirmRootPassword: string;
  email: string;
}

interface FormErrors {
  username?: string;
  password?: string;
  confirmPassword?: string;
  rootPassword?: string;
  confirmRootPassword?: string;
  email?: string;
}

export default function SetupWizard() {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const { isSubmitting, setupComplete, error, createdUsername } = useAppSelector(
    (state) => state.setup
  );

  const [formData, setFormData] = useState<FormData>({
    username: "",
    password: "",
    confirmPassword: "",
    rootPassword: "",
    confirmRootPassword: "",
    email: "",
  });

  const [formErrors, setFormErrors] = useState<FormErrors>({});
  const [showPassword, setShowPassword] = useState(false);
  const [showRootPassword, setShowRootPassword] = useState(false);
  const [step, setStep] = useState<1 | 2 | 3>(1);

  // Clear error when form data changes
  useEffect(() => {
    if (error) {
      dispatch(clearSetupError());
    }
  }, [formData, dispatch]);

  // Navigate to login after successful setup
  useEffect(() => {
    if (setupComplete) {
      const timer = setTimeout(() => {
        navigate("/login", { replace: true });
      }, 3000);
      return () => clearTimeout(timer);
    }
  }, [setupComplete, navigate]);

  const validateStep1 = (): boolean => {
    const errors: FormErrors = {};
    
    if (!formData.username.trim()) {
      errors.username = "Username is required";
    } else if (formData.username.toLowerCase() === "root") {
      errors.username = "Cannot use 'root' as username";
    } else if (formData.username.length < 3) {
      errors.username = "Username must be at least 3 characters";
    }

    if (formData.email && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(formData.email)) {
      errors.email = "Invalid email format";
    }

    setFormErrors(errors);
    return Object.keys(errors).length === 0;
  };

  const validateStep2 = (): boolean => {
    const errors: FormErrors = {};

    if (!formData.password) {
      errors.password = "Password is required";
    } else if (formData.password.length < 8) {
      errors.password = "Password must be at least 8 characters";
    }

    if (formData.password !== formData.confirmPassword) {
      errors.confirmPassword = "Passwords do not match";
    }

    setFormErrors(errors);
    return Object.keys(errors).length === 0;
  };

  const validateStep3 = (): boolean => {
    const errors: FormErrors = {};

    if (!formData.rootPassword) {
      errors.rootPassword = "Root password is required";
    } else if (formData.rootPassword.length < 8) {
      errors.rootPassword = "Root password must be at least 8 characters";
    }

    if (formData.rootPassword !== formData.confirmRootPassword) {
      errors.confirmRootPassword = "Root passwords do not match";
    }

    setFormErrors(errors);
    return Object.keys(errors).length === 0;
  };

  const handleNext = () => {
    if (step === 1 && validateStep1()) {
      setStep(2);
    } else if (step === 2 && validateStep2()) {
      setStep(3);
    }
  };

  const handleBack = () => {
    if (step === 2) setStep(1);
    else if (step === 3) setStep(2);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!validateStep3()) return;

    dispatch(
      submitSetup({
        username: formData.username,
        password: formData.password,
        root_password: formData.rootPassword,
        email: formData.email || undefined,
      })
    );
  };

  const handleChange = (field: keyof FormData) => (e: React.ChangeEvent<HTMLInputElement>) => {
    setFormData((prev) => ({ ...prev, [field]: e.target.value }));
    // Clear specific field error
    if (formErrors[field]) {
      setFormErrors((prev) => ({ ...prev, [field]: undefined }));
    }
  };

  // Success screen
  if (setupComplete) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center bg-gradient-to-br from-background to-muted p-4">
        <Card className="w-full max-w-md">
          <CardHeader className="text-center">
            <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-green-100 dark:bg-green-900">
              <Check className="h-8 w-8 text-green-600 dark:text-green-400" />
            </div>
            <CardTitle className="text-2xl">Setup Complete!</CardTitle>
            <CardDescription className="mt-2">
              Your KalamDB server has been configured successfully.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4 text-center">
            <div className="rounded-lg bg-muted p-4">
              <p className="text-sm text-muted-foreground">
                DBA user <strong className="text-foreground">{createdUsername}</strong> has been created.
              </p>
              <p className="mt-2 text-sm text-muted-foreground">
                Root password has been configured.
              </p>
            </div>
            <p className="text-sm text-muted-foreground">
              Redirecting to login in 3 seconds...
            </p>
            <Button onClick={() => navigate("/login", { replace: true })} className="w-full">
              Go to Login Now
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-gradient-to-br from-background to-muted p-4">
      <div className="mb-8 flex items-center gap-3">
        <Database className="h-12 w-12 text-primary" />
        <div>
          <h1 className="text-3xl font-bold">KalamDB</h1>
          <p className="text-sm text-muted-foreground">Server Setup</p>
        </div>
      </div>

      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>
            {step === 1 && "Create Your Account"}
            {step === 2 && "Set Your Password"}
            {step === 3 && "Set Root Password"}
          </CardTitle>
          <CardDescription>
            {step === 1 && "Enter your username and optional email for your DBA account."}
            {step === 2 && "Choose a secure password for your DBA account."}
            {step === 3 && "Set a root password for system administration."}
          </CardDescription>
          
          {/* Progress indicator */}
          <div className="mt-4 flex items-center justify-center gap-2">
            {[1, 2, 3].map((s) => (
              <div
                key={s}
                className={`h-2 w-12 rounded-full transition-colors ${
                  s <= step ? "bg-primary" : "bg-muted"
                }`}
              />
            ))}
          </div>
        </CardHeader>

        <CardContent>
          <form onSubmit={step === 3 ? handleSubmit : (e) => { e.preventDefault(); handleNext(); }}>
            {/* Step 1: Username & Email */}
            {step === 1 && (
              <div className="space-y-4">
                <div className="space-y-2">
                  <label htmlFor="username" className="text-sm font-medium">
                    Username <span className="text-destructive">*</span>
                  </label>
                  <Input
                    id="username"
                    type="text"
                    placeholder="Enter username"
                    value={formData.username}
                    onChange={handleChange("username")}
                    className={formErrors.username ? "border-destructive" : ""}
                    autoFocus
                  />
                  {formErrors.username && (
                    <p className="text-sm text-destructive">{formErrors.username}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <label htmlFor="email" className="text-sm font-medium">
                    Email <span className="text-muted-foreground">(optional)</span>
                  </label>
                  <Input
                    id="email"
                    type="email"
                    placeholder="Enter email"
                    value={formData.email}
                    onChange={handleChange("email")}
                    className={formErrors.email ? "border-destructive" : ""}
                  />
                  {formErrors.email && (
                    <p className="text-sm text-destructive">{formErrors.email}</p>
                  )}
                </div>
              </div>
            )}

            {/* Step 2: Password */}
            {step === 2 && (
              <div className="space-y-4">
                <div className="space-y-2">
                  <label htmlFor="password" className="text-sm font-medium">
                    Password <span className="text-destructive">*</span>
                  </label>
                  <div className="relative">
                    <Input
                      id="password"
                      type={showPassword ? "text" : "password"}
                      placeholder="Enter password"
                      value={formData.password}
                      onChange={handleChange("password")}
                      className={formErrors.password ? "border-destructive pr-10" : "pr-10"}
                      autoFocus
                    />
                    <button
                      type="button"
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                      onClick={() => setShowPassword(!showPassword)}
                    >
                      {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                  </div>
                  {formErrors.password && (
                    <p className="text-sm text-destructive">{formErrors.password}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <label htmlFor="confirmPassword" className="text-sm font-medium">
                    Confirm Password <span className="text-destructive">*</span>
                  </label>
                  <Input
                    id="confirmPassword"
                    type={showPassword ? "text" : "password"}
                    placeholder="Confirm password"
                    value={formData.confirmPassword}
                    onChange={handleChange("confirmPassword")}
                    className={formErrors.confirmPassword ? "border-destructive" : ""}
                  />
                  {formErrors.confirmPassword && (
                    <p className="text-sm text-destructive">{formErrors.confirmPassword}</p>
                  )}
                </div>
              </div>
            )}

            {/* Step 3: Root Password */}
            {step === 3 && (
              <div className="space-y-4">
                <div className="rounded-lg bg-amber-50 dark:bg-amber-950/30 p-3 text-sm text-amber-800 dark:text-amber-200">
                  <div className="flex items-start gap-2">
                    <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    <p>
                      The root password is used for system administration. Keep it safe and secure.
                    </p>
                  </div>
                </div>

                <div className="space-y-2">
                  <label htmlFor="rootPassword" className="text-sm font-medium">
                    Root Password <span className="text-destructive">*</span>
                  </label>
                  <div className="relative">
                    <Input
                      id="rootPassword"
                      type={showRootPassword ? "text" : "password"}
                      placeholder="Enter root password"
                      value={formData.rootPassword}
                      onChange={handleChange("rootPassword")}
                      className={formErrors.rootPassword ? "border-destructive pr-10" : "pr-10"}
                      autoFocus
                    />
                    <button
                      type="button"
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                      onClick={() => setShowRootPassword(!showRootPassword)}
                    >
                      {showRootPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                  </div>
                  {formErrors.rootPassword && (
                    <p className="text-sm text-destructive">{formErrors.rootPassword}</p>
                  )}
                </div>

                <div className="space-y-2">
                  <label htmlFor="confirmRootPassword" className="text-sm font-medium">
                    Confirm Root Password <span className="text-destructive">*</span>
                  </label>
                  <Input
                    id="confirmRootPassword"
                    type={showRootPassword ? "text" : "password"}
                    placeholder="Confirm root password"
                    value={formData.confirmRootPassword}
                    onChange={handleChange("confirmRootPassword")}
                    className={formErrors.confirmRootPassword ? "border-destructive" : ""}
                  />
                  {formErrors.confirmRootPassword && (
                    <p className="text-sm text-destructive">{formErrors.confirmRootPassword}</p>
                  )}
                </div>
              </div>
            )}

            {/* API Error */}
            {error && (
              <div className="mt-4 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
                <div className="flex items-start gap-2">
                  <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
                  <p>{error}</p>
                </div>
              </div>
            )}

            {/* Navigation buttons */}
            <div className="mt-6 flex gap-3">
              {step > 1 && (
                <Button type="button" variant="outline" onClick={handleBack} className="flex-1">
                  Back
                </Button>
              )}
              {step < 3 ? (
                <Button type="submit" className="flex-1">
                  Continue
                </Button>
              ) : (
                <Button type="submit" className="flex-1" disabled={isSubmitting}>
                  {isSubmitting ? (
                    <>
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      Setting up...
                    </>
                  ) : (
                    "Complete Setup"
                  )}
                </Button>
              )}
            </div>
          </form>
        </CardContent>
      </Card>

      <p className="mt-6 text-center text-sm text-muted-foreground max-w-md">
        This wizard will create your initial DBA account and configure the root password 
        for system administration.
      </p>
    </div>
  );
}
