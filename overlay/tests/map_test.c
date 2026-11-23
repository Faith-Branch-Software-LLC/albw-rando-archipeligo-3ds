// Host test: runs the plugin's logic over a map file and compares it with the randomizer's.
// usage: map_test <albw-map.bin> <case file>...
// A case file holds one line per check: whether it is checked (0 or 1), then the status the
// randomizer expects.
#include "../map_logic.h"
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv)
{
    FILE *file = fopen(argv[1], "rb");
    if (!file)
        return 2;
    fseek(file, 0, SEEK_END);
    long size = ftell(file);
    fseek(file, 0, SEEK_SET);
    u8 *data = malloc(size);
    fread(data, 1, size, file);
    fclose(file);
    MapData map;
    if (map_parse(&map, data, size) != 0)
    {
        printf("could not read the map file\n");
        return 1;
    }
    int failures = 0;
    for (int c = 2; c < argc; c++)
    {
        FILE *cases = fopen(argv[c], "r");
        u8 *checked = calloc(map.checkCount, 1), *expected = calloc(map.checkCount, 1), *status = calloc(map.checkCount, 1);
        for (u32 i = 0; i < map.checkCount; i++)
        {
            int a, b;
            if (fscanf(cases, "%d %d", &a, &b) != 2)
                return 3;
            checked[i] = a;
            expected[i] = b;
        }
        fclose(cases);
        map_evaluate(&map, checked, status);
        int wrong = 0, in = 0;
        for (u32 i = 0; i < map.checkCount; i++)
        {
            in += status[i] == STATUS_IN_LOGIC;
            if (status[i] != expected[i])
            {
                if (wrong++ < 5)
                    printf("  %s: plugin %d, randomizer %d\n", map_check(&map, i).name, status[i], expected[i]);
            }
        }
        printf("%s: %u checks, %d in logic, %d differ\n", argv[c], map.checkCount, in, wrong);
        failures += wrong != 0;
    }
    return failures != 0;
}
