import { expect, it } from "vitest";

import { repositoryLink } from "./repositoryLink";

it.each([
  ["https://user:secret@github.com/acme/app.git?token=secret#main", "https://github.com/acme/app", "GitHub", "github"],
  ["git://github.com:9418/acme/app.git", "https://github.com/acme/app", "GitHub", "github"],
  ["git+ssh://git@gitlab.com:2222/group/app.git", "https://gitlab.com/group/app", "GitLab", "gitlab"],
  ["ssh+git://git@bitbucket.org:22/acme/app.git", "https://bitbucket.org/acme/app", "Bitbucket", "bitbucket"],
  ["git@github.com:/acme/app.git", "https://github.com/acme/app", "GitHub", "github"],
  ["git@github.com:acme/app.git", "https://github.com/acme/app", "GitHub", "github"],
  ["ssh://git@gitlab.com:2222/group/subgroup/app.git", "https://gitlab.com/group/subgroup/app", "GitLab", "gitlab"],
  ["git@bitbucket.org:acme/app.git/", "https://bitbucket.org/acme/app", "Bitbucket", "bitbucket"],
  ["http://git.example.org:8080/team/app.git", "http://git.example.org:8080/team/app", "git.example.org", undefined],
  ["https://github.com.evil.example/acme/app.git", "https://github.com.evil.example/acme/app", "github.com.evil.example", undefined],
  ["ssh://git@code.example.org/team/app", "https://code.example.org/team/app", "code.example.org", undefined],
])("turns %s into a credential-free repository web link", (remote, url, host, brand) => {
  expect(repositoryLink(remote)).toEqual({ url, host, ...(brand ? { brand } : {}) });
});

it.each([undefined, null, "", "/tmp/repo", "../repo", "C:/repo", "file:///tmp/repo", "javascript:alert(1)", "https://github.com", "https://github.com/", "https://git hub.com/acme/app", "https://github.com\\@evil.example/app", "localhost:repo", "ssh://%zz/repo", "ssh://git@a^b/repo", "https://github.com/a/\u0000repo", "https://github.com/a/\u007frepo"])("omits an absent, local or unsupported remote: %s", (remote) => {
  expect(repositoryLink(remote)).toBeUndefined();
});
