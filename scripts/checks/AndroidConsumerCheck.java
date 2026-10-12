// Compare JNA's reflection-visible ABI before and after shrinking the real AAR.
// This checker runs outside R8 so its reflection cannot implicitly keep members.
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.TreeSet;
import java.util.zip.ZipFile;

public class AndroidConsumerCheck {
    private static String signature(Class<?>[] parameters) {
        return Arrays.toString(Arrays.stream(parameters).map(Class::getName).toArray());
    }

    private static TreeSet<String> members(Class<?> type, boolean structure) throws Exception {
        TreeSet<String> result = new TreeSet<>();
        for (Constructor<?> constructor : type.getConstructors()) {
            result.add("constructor " + signature(constructor.getParameterTypes()));
        }
        if (structure) {
            for (Field field : type.getFields()) {
                if (!Modifier.isStatic(field.getModifiers())) {
                    result.add("field " + field.getName() + " " + field.getType().getName());
                }
            }
            for (var annotation : type.getAnnotations()) {
                if (annotation.annotationType().getName().equals("com.sun.jna.Structure$FieldOrder")) {
                    result.add("field order " + Arrays.toString((String[]) annotation.annotationType()
                        .getMethod("value").invoke(annotation)));
                }
            }
        } else {
            for (Method method : type.getDeclaredMethods()) {
                if (Modifier.isPublic(method.getModifiers())) {
                    result.add("method " + method.getName() + signature(method.getParameterTypes())
                        + " " + method.getReturnType().getName());
                }
            }
        }
        return result;
    }

    public static void main(String[] args) throws Exception {
        URL[] beforePaths = Arrays.stream(args[2].split(":"))
            .map(Path::of).map(Path::toUri).map(uri -> {
                try { return uri.toURL(); } catch (Exception e) { throw new RuntimeException(e); }
            }).toArray(URL[]::new);
        URL[] afterPaths = beforePaths.clone();
        beforePaths[0] = Path.of(args[0]).toUri().toURL();
        afterPaths[0] = Path.of(args[1]).toUri().toURL();
        afterPaths[1] = afterPaths[0];
        try (var original = new URLClassLoader(beforePaths, ClassLoader.getPlatformClassLoader());
             var minified = new URLClassLoader(afterPaths, ClassLoader.getPlatformClassLoader());
             var archive = new ZipFile(args[0])) {
            Class<?> structure = original.loadClass("com.sun.jna.Structure");
            Class<?> library = original.loadClass("com.sun.jna.Library");
            Class<?> callback = original.loadClass("com.sun.jna.Callback");
            int structures = 0, libraries = 0, callbacks = 0;
            for (var entry : archive.stream().filter(e -> e.getName().endsWith(".class")).toList()) {
                String name = entry.getName().replace('/', '.').replaceAll("\\.class$", "");
                Class<?> type = original.loadClass(name);
                boolean isStructure = structure.isAssignableFrom(type);
                boolean isLibrary = library.isAssignableFrom(type);
                boolean isCallback = callback.isAssignableFrom(type);
                if (!(isStructure || isLibrary || isCallback)) continue;
                Class<?> shrunk;
                try { shrunk = minified.loadClass(name); }
                catch (ClassNotFoundException e) {
                    throw new AssertionError("Minification removed or renamed JNA class " + name, e);
                }
                var expected = members(type, isStructure);
                var actual = members(shrunk, isStructure);
                if (!expected.equals(actual)) {
                    throw new AssertionError("Minification changed JNA members of " + name
                        + "\nExpected: " + expected + "\nActual: " + actual);
                }
                if (isStructure) structures++;
                if (isLibrary) libraries++;
                if (isCallback) callbacks++;
            }
            if (structures == 0 || libraries == 0 || callbacks == 0) {
                throw new AssertionError("Consumer check did not exercise all JNA ABI categories");
            }
            System.out.printf("R8 preserved %d structures, %d native interfaces and %d callbacks.%n",
                structures, libraries, callbacks);
        }
    }
}
