import type { Meta, StoryObj } from "@storybook/react"
import { MockedProvider } from "@apollo/client/testing"

import UserPage from "./page"

import faker from "@/.storybook/faker"
import { GetUserDetailsDocument, Role } from "@/lib/graphql/generated"

import { mockUser } from "@/lib/graphql/generated/mocks"

interface UserStoryArgs {
  email: string
  roles: Role[]
}

const DEFAULT_ARGS: UserStoryArgs = {
  email: faker.internet.email(),
  roles: [Role.Admin],
}

const createMocks = (args: UserStoryArgs, userId: string) => [
  {
    request: {
      query: GetUserDetailsDocument,
      variables: { id: userId },
    },
    result: {
      data: {
        user: mockUser({
          email: args.email,
          roles: args.roles,
          userId: userId,
          createdAt: faker.date.past().toISOString(),
        }),
      },
    },
  },
]

const UserStory = (args: UserStoryArgs) => {
  const userId = faker.string.uuid()
  const mocks = createMocks(args, userId)
  return (
    <MockedProvider mocks={mocks} addTypename={false} key={JSON.stringify(args)}>
      <UserPage params={Promise.resolve({ "user-id": userId })} />
    </MockedProvider>
  )
}

const LoadingStory = () => {
  const userId = faker.string.uuid()
  const mocks = [
    {
      request: {
        query: GetUserDetailsDocument,
        variables: { id: userId },
      },
      delay: Infinity,
    },
  ]

  return (
    <MockedProvider mocks={mocks} addTypename={false}>
      <UserPage params={Promise.resolve({ "user-id": userId })} />
    </MockedProvider>
  )
}

const meta: Meta<typeof UserStory> = {
  title: "Pages/Users/User/Details",
  component: UserStory,
  parameters: {
    layout: "fullscreen",
    nextjs: { appDirectory: true },
  },
  argTypes: {
    email: {
      control: "text",
      description: "User email",
    },
    roles: {
      control: "multi-select",
      options: Object.values(Role),
      description: "User roles",
    },
  },
}

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: DEFAULT_ARGS,
}

export const MultiRole: Story = {
  args: {
    ...DEFAULT_ARGS,
    roles: [Role.Admin, Role.BankManager],
  },
}

export const Loading: Story = {
  args: DEFAULT_ARGS,
  render: LoadingStory,
  parameters: {
    nextjs: {
      navigation: {
        pathname: "/users/[user-id]",
      },
    },
  },
}
